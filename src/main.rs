#[macro_use] extern crate prettytable;
use confy;
use data_types::{AppState, AppConfig, AppCommand, AppElement};
use std::fs;
use std::io;
use std::path::PathBuf;
use clap::{Arg, Command, ArgMatches, crate_authors, crate_description, crate_version, ArgAction};
use dialoguer::{Input, Confirm, Password, FuzzySelect, Select, theme::ColorfulTheme, console::Term};

use crate::data_types::AuthMethod;

pub(crate) mod data_types {
    use std::{fmt, io::Cursor, str};
    use chrono::{TimeZone, Utc, LocalResult};
    use prettytable::{Table, Row};
    use serde::{Serialize, Deserialize};
    use reqwest::{Client, Response, header::HeaderValue};
    use quick_xml::{de::from_str, Reader, events::{attributes::Attribute, Event, BytesStart, BytesText, BytesEnd}, Writer};
    use rand::Rng;
    //use http::uri;
    
    #[derive(Serialize, Deserialize)]
    struct Registry {
        #[serde(rename = "entry")]
        entries: Vec<AppElement>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename = "entry")]
    pub struct AppElement {
        #[serde(rename = "@id")]
        id: Option<u16>,
        #[serde(rename = "name")]
        title: String,
        description: String,
        due: Option<u32>,
        #[serde(skip)]
        removed: bool,
    }

    impl PartialEq for AppElement {
        fn eq(&self, other: &AppElement) -> bool {
            match self.id {
                Some(id) => Some(id) == other.id,
                None => self == other, // Isn't this recursive???
            }
        }
    }

    impl fmt::Display for AppElement {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let disp_due: String = match self.due {
                Some(due) => {
                    let due_timestamp: i64 = due.into();
                    let utc_due: String = match Utc.timestamp_opt(due_timestamp, 0) {
                        LocalResult::None => "None".to_string(),
                        LocalResult::Single(val) => val.to_rfc2822(),
                        LocalResult::Ambiguous(val, _) => val.to_rfc2822(),
                    };
                    utc_due
                },
                None => "None".to_string()
            };

            let id: String = match self.id {
                Some(id) => format!("{}", id),
                None => "None".to_string()
            };
            write!(
                f,
                "ID: {}\nTitle: {}\nDescription: {}\nDue: {:#?}\n",
                id,
                &self.title,
                &self.description,
                disp_due
            )
        }
    }

    impl AppElement {
        pub fn new(id: Option<u16>, title: String, description: String, due: Option<u32>) -> Self{
            Self {
                id,
                title,
                description,
                due,
                removed: false,
            }
        }

        /// Generates a new ID for this element. The id will not be in existing ids
        /// Updates the self element and the existing ids
        /// Returns the new id
        pub fn generate_id(&mut self, existing_ids: &mut Vec<u16>) -> u16 {
            let mut rng = rand::thread_rng();
            let mut new_id: u16 = 0;
            while new_id == 0 || existing_ids.iter().any(|&i| i==new_id) {
                new_id = rng.gen::<u16>();
            }
            self.id = Some(new_id);
            existing_ids.push(new_id);
            new_id
        }

        /// Writes the element using the given quick xml writer
        /// skips silently if the element does not have an ID
        pub fn write<W: std::io::Write>(&self, writer: &mut Writer<W>) -> Result<(), quick_xml::Error> {
            if self.id.is_none() {
                return Ok(());
            }
            writer.write_event(Event::Start(
                BytesStart::new("entry")
                    .with_attributes([Attribute::from(("id", self.id.unwrap().to_string().as_str()))])
                )
            )?;
            writer.write_event(Event::Start(BytesStart::new("name")))?;
            writer.write_event(Event::Text(BytesText::new(&self.title)))?;
            writer.write_event(Event::End(BytesEnd::new("name")))?;

            writer.write_event(Event::Start(BytesStart::new("description")))?;
            writer.write_event(Event::Text(BytesText::new(&self.description)))?;
            writer.write_event(Event::End(BytesEnd::new("description")))?;

            if self.due.is_some() {
                writer.write_event(Event::Start(BytesStart::new("due")))?;
                writer.write_event(Event::Text(BytesText::new(&self.due.unwrap().to_string())))?;
                writer.write_event(Event::End(BytesEnd::new("due")))?;
            }

            writer.write_event(Event::End(BytesEnd::new("entry")))?;

            Ok(())
        }

        fn to_row(&self) -> Row {
            let disp_due: String = match self.due {
                Some(due) => {
                    let due_timestamp: i64 = due.into();
                    let utc_due: String = match Utc.timestamp_opt(due_timestamp, 0) {
                        LocalResult::None => "None".to_string(),
                        LocalResult::Single(val) => val.to_rfc2822(),
                        LocalResult::Ambiguous(val, _) => val.to_rfc2822(),
                    };
                    //let offset = Local::now().offset();
                    utc_due

                },
                None => "None".to_string(),
            };

            let id = match self.id {
                Some(id) => format!("{}", id),
                None => "None".to_string()
            };
            if self.removed {
                row![
                    Fri =>
                    id,
                    self.title,
                    self.description,
                    disp_due,
                ]
            } else if self.id.is_none() {
                row![
                    Fbi =>
                    id,
                    self.title,
                    self.description,
                    disp_due,
                ]
            } else {
                row![
                    id,
                    self.title,
                    self.description,
                    disp_due,
                ]
            }
        }
    }

    pub struct AppState {
        config: AppConfig,
        client: Option<Client>,
        elements: Vec<AppElement>,
        synced: bool,
    }

    impl AppState {
        pub fn new(config: AppConfig) -> Self {
            Self {
                config,
                client: None,
                elements: Vec::new(),
                synced: false,
            }
        }

        pub fn get_elements(&self) -> &Vec<AppElement> {
            return &self.elements;
        }

        pub fn get_ids(&self, ignore_removed: bool) -> Vec<u16> {
            return self.elements
                .clone()
                .into_iter()
                .filter(|e| !e.removed && ignore_removed)
                .filter_map(|e| e.id)
                .collect();

        }

        pub fn push(&mut self, element: Option<AppElement>) {
            if let Some(e) = element {
                self.elements.push(e)
            }
        }

        pub fn unsynced(&mut self) {
            self.synced = false;
        }

        fn handle_empty_client(&mut self) {
            if self.client.is_none() {
                self.client = Some(
                    Client::builder()
                        .user_agent("Freemind CLI")
                        .build().unwrap()
                );
            }
        }

        /// Adds non existing elements to the State of elements, skips
        /// already existing elements
        fn add_new_elements(&mut self, new: Vec<AppElement>) {
            new.into_iter().for_each(|e| {
                if self.elements.iter().any(|i| &e == i) {

                } else {
                    self.elements.push(e)
                }
            })
        }

        fn add_missing_ids(&mut self, existing_ids: &mut Vec<u16>) -> (bool, Vec<u16>) {
            let mut new_ids: Vec<u16> = Vec::new();
            let count_before: usize = self.elements.len();
            let count_after: usize = self.elements
                .iter_mut()
                .filter(|e| e.id.is_none())
                .map(|e| {
                    new_ids.push(e.generate_id(existing_ids))
                }).count();
            (count_before != count_after, new_ids)
        }

        async fn fetch(&mut self) -> Result<String, reqwest::Error> {
            self.handle_empty_client();
            let res: Response = self.client.as_ref().unwrap()
                .post(format!("{}/xml/fetch", self.config.server_address))
                .header(
                    "user".to_string(),
                    HeaderValue::from_str(&self.config.username).unwrap()
                )
                .header(
                    format!("{}", &self.config.auth_method).to_lowercase(),
                    &self.config.secret
                )
                .send()
                .await?;

            let headers = res.headers();
            if headers.get("content-type") == Some(&HeaderValue::from_static("text/xml")) {
                let txt = res.text().await?;
                return Ok(txt);
            }

            Ok(String::new())
        }

        /// Uploads the given payload to the server and returns the HTTP status code
        async fn upload(&mut self, payload: String) -> Result<u16, reqwest::Error> {
            self.handle_empty_client();
            let res: Response = self.client.as_ref().unwrap()
                .post(format!("{}/xml/update", self.config.server_address))
                .header(
                    "user".to_string(),
                    HeaderValue::from_str(&self.config.username).unwrap()
                )
                .header(
                    format!("{}", &self.config.auth_method).to_lowercase(),
                    &self.config.secret
                )
                .header(
                    "content-type".to_string(),
                    "text/xml".to_string(),
                )
                .body(payload)
                .send()
                .await?;

            let status = res.status().as_u16();

            return Ok(status)
        }

        /// Takes the whole XML Document and removes all Entries that were removed
        /// in the internal state.
        /// Returns whether changes where made and the string of the new payload
        fn delete_removed(&mut self, xml: String) -> Result<(bool, String), reqwest::Error> {
            let mut modified = false;

            let mut reader = Reader::from_str(&xml);
            let mut writer = Writer::new(Cursor::new(Vec::new()));
            
            reader.trim_text(true);

            let mut ffwd: bool = false;
            let mut skip: BytesStart = BytesStart::new("");

            loop {
                match reader.read_event() {
                    Ok(Event::Start(_)) if ffwd => {
                        continue
                    }
                    Ok(Event::Start(e)) if e.name().as_ref() == b"entry" => {
                        let mut write = true;
                        e
                            .attributes()
                            .into_iter()
                            .filter_map(|f| f.ok())
                            .for_each(|val| {
                                if val.key.local_name().as_ref() == b"id" {
                                    if let Ok(v) = val.decode_and_unescape_value(&reader) {
                                        if let Ok(v) = v.to_string().parse::<u16>() {
                                            if let Some(pos) = self.elements.iter().position(|e| e.id == Some(v)) {
                                                if pos < self.elements.len() && self.elements[pos].removed {
                                                    self.elements.remove(pos);
                                                    ffwd = true;
                                                    skip = e.to_owned();
                                                    modified = true;
                                                    write = false;
                                                };
                                            };
                                        };
                                    };
                                };
                            });
                        if write {
                            writer.write_event(Event::Start(e.to_owned())).unwrap();
                        }
                    },
                    Ok(Event::Start(e)) => {
                        writer.write_event(Event::Start(e.to_owned())).unwrap();
                    }
                    Ok(Event::End(e)) if e == skip.to_end() => {
                        ffwd = false;
                        skip = BytesStart::new("");
                    }
                    Ok(Event::End(_)) if ffwd => {
                        continue
                    }
                    Ok(Event::End(e)) => {
                        writer.write_event(Event::End(e.to_owned())).unwrap();
                    },
                    Ok(Event::Eof) => break,
                    Ok(_) if ffwd => {
                        continue
                    },
                    Ok(e) => {
                        writer.write_event(e).unwrap();
                    }
                    Err(_) => break,
                    //_ => (),
                }
            }

            Ok((
                modified,
                str::from_utf8(
                    &writer.into_inner().into_inner()
                )
                .unwrap()
                .to_string()
            ))
        }

        /// Takes the whole XML Document and inserts Entries defined by the ids vec into it
        fn insert_created_entries(&self, xml: String, ids: Vec<u16>) -> String {
            let mut reader = Reader::from_str(&xml);
            let mut writer = Writer::new(Cursor::new(Vec::new()));

            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) if e.name().as_ref() == b"registry" => {
                        writer.write_event(Event::Start(e.to_owned())).unwrap();
                        self.elements
                            .iter()
                            .filter(|e| ids.iter().any(|i| Some(i) == e.id.as_ref()))
                            .map(|e| {
                                e.write(&mut writer).unwrap();
                            }).count();
                    },
                    Ok(Event::Eof) => break,
                    Ok(e) => {writer.write_event(e).unwrap();}
                    Err(_) => break,
                }
            }

            str::from_utf8(
                &writer
                .into_inner()
                .into_inner()
            ).unwrap().to_string()
        }

        pub fn is_synced(&self) -> bool {
            self.synced
        }
        pub fn list(&self) {
            let mut table = Table::new();
            table.set_titles(row!["ID", "Title", "Description", "Due"]);
            self.elements.iter().for_each(|e| {
                table.add_row(e.to_row());
            });
            table.printstd();
        }

        /// Syncs changes, fetches new elements, deletes removed elements and pushes
        pub async fn sync(&mut self) -> Result<(), reqwest::Error> {
            println!("Fetching new Entries...");
            let result = self.fetch().await?;

            println!("Evaluating State...");
            let (entries_deleted, mut answer) = self.delete_removed(result.to_string())?;

            let fetched_registry: Registry = from_str(&answer).unwrap();

            let mut existing_ids: Vec<u16> = fetched_registry.entries
                .clone()
                .into_iter()
                .filter(|e| !e.removed)
                .filter_map(|e| e.id)
                .collect();

            let (entries_added, new_ids) = self.add_missing_ids(&mut existing_ids);

            if entries_added {
                answer = self.insert_created_entries(answer, new_ids);
            }

            let needs_upload: bool = entries_deleted || entries_added;

            if needs_upload {
                println!("Uploading Changes...");
                self.upload(answer.clone()).await?;
            }


            self.add_new_elements(fetched_registry.entries);

            self.synced = true;
            println!("Done!");
            Ok(())
        }

        pub fn remove(&mut self, id: u16) -> bool {
            let Some(posi) = self.elements.iter().position(|e| e.id == Some(id)) else {return false};
            //self.elements.remove(found_index);
            self.elements[posi].removed = true;
            true
        }
    }

    #[derive(PartialEq,)]
    pub enum AppCommand {
        List,
        Sync,
        Filter,
        Edit,
        Add,
        Remove,
        Help,
        Quit,
    }

    impl ToString for AppCommand {
        fn to_string(&self) -> String {
            match self {
                Self::List => "list",
                Self::Sync => "sync",
                Self::Filter => "filter",
                Self::Edit => "edit",
                Self::Add => "add",
                Self::Remove => "remove",
                Self::Help => "help",
                Self::Quit => "quit",
            }.to_string()
        }
    }

    impl From<usize> for AppCommand {
        fn from(s: usize) -> Self {
            match s {
                0 => Self::List,
                1 => Self::Sync,
                2 => Self::Filter,
                3 => Self::Edit,
                4 => Self::Add,
                5 => Self::Remove,
                6 => Self::Help,
                7 => Self::Quit,
                _ => Self::List
            }
        }
    }

    impl AppCommand {
        pub fn get_command_list() -> Vec<AppCommand> {
            vec![
                Self::List,
                Self::Sync,
                Self::Filter,
                Self::Edit,
                Self::Add,
                Self::Remove,
                Self::Help,
                Self::Quit
            ]
        }
    }

    #[derive(Serialize, Deserialize, PartialEq)]
    pub enum AuthMethod {
        Token,
        Password
    }

    impl From<usize> for AuthMethod {
        fn from(s: usize) -> AuthMethod {
            match s {
                0 => AuthMethod::Token,
                1 => AuthMethod::Password,
                _ => AuthMethod::Token,
            }
        }
    }

    impl fmt::Display for AuthMethod {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> fmt::Result {
            let displ: &str = match self {
                AuthMethod::Token => "Token",
                AuthMethod::Password => "Password",
            };
            write!(f, "{}", displ)
        }
    }

    #[derive(Serialize, Deserialize, PartialEq)]
    pub struct AppConfig {
        pub server_address: String,
        pub username: String,
        pub secret: String,
        pub auth_method: AuthMethod,
    }

    /// Construct a default AppConfig
    impl ::std::default::Default for AppConfig {
        fn default() -> Self {
            Self {
                server_address: "<THE ADDRESS OF THE WEBSERVER>".to_string(),
                username: "<YOUR USERNAME>".to_string(),
                secret: "<YOUR TOKEN / SECRET>".to_string(),
                auth_method: AuthMethod::Token,
            }
        }
    }

    impl fmt::Display for AppConfig {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(
                f,
                "Server: {}\nUsername: {}\nSecret: {}\nAuth Method: {}",
                self.server_address, self.username, "*".repeat(self.secret.len()), self.auth_method
            )
        }
    }

    impl AppConfig {
        /// Returns if the element is the same as the default options
        pub(crate) fn is_default(&self) -> bool {
            self == &Self::default()
        }

        /// Returns if the element is the same as the empty element
        pub(crate) fn is_empty(&self) -> bool {
            self == &Self::empty()
        }

        /// Returns a minimal element
        pub(crate) fn empty() -> Self {
            Self {
                server_address: "".to_string(),
                username: "".to_string(),
                secret: "".to_string(),
                auth_method: AuthMethod::Token,
            }
        }

        pub(crate) fn new(server_address: String, username: String, secret: String, auth_method: AuthMethod) -> Self {
            Self {
                server_address,
                username,
                secret,
                auth_method,
            }
        }
    }
}

/// Read the app configuration
fn obtain_app_config() -> Option<AppConfig> {
    let mut path = dirs::config_dir().unwrap_or(PathBuf::new());
    path.push("freemind/");
    fs::create_dir_all(path.clone()).ok();
    path.push("freemind-cli.config");
    confy::load_path(path).ok()
}

/// Save the app configuration
fn write_app_config(config: &AppConfig) -> Option<()> {
    let mut path = dirs::config_dir().unwrap_or(PathBuf::new());
    path.push("freemind/");
    fs::create_dir_all(path.clone()).ok();
    path.push("freemind-cli.config");
    confy::store_path(path, config).ok();
    Some(())
}

/// Configuration Setup Dialog
fn setup_config(prev_config: &AppConfig) -> Result<AppConfig, std::io::Error> {
    println!("\n   ### Config Setup: ###\n");
    let server_address: String = Input::new()
        .with_prompt("URL of the server to connect to")
        .with_initial_text(&prev_config.server_address)
        .interact_text()?;

    let username: String = Input::new()
        .with_prompt("Your username")
        .with_initial_text(&prev_config.username)
        .interact_text()?;

    let auth_method: AuthMethod = AuthMethod::from(Select::with_theme(&ColorfulTheme::default())
        .with_prompt("How do you want to authenticate?")
        .items(&vec!["API Token", "Password"])
        .default(0)
        .interact_on_opt(&Term::stderr())?.unwrap_or(0));

    let secret: String = match auth_method {
        AuthMethod::Token => Input::new()
            .with_prompt("Your API Token")
            .interact_text()?,
        AuthMethod::Password => Password::new()
            .with_prompt("Your Password")
            .interact()?
    };

    let config: AppConfig = AppConfig::new(
        server_address,
        username,
        secret,
        auth_method,
    );

    println!("\nDone! You entered the following config:\n\n{}\n", config);
    if Confirm::new().with_prompt("Do you want to accept this config?").interact()? {
        return Ok(config);
    } else {
        println!("\n");
        return setup_config(&config);
    }

}


fn filter_menu() {
    println!("The Filter Menu is currently not implemented");
}

fn edit_menu() {
    println!("The Edit Menu is currently not implemented");
}

fn add_menu(state: &mut AppState) -> Result<(), std::io::Error> {
    let title: String = Input::new()
        .with_prompt("Title")
        .interact_text()?;

    let description: String = Input::new()
        .with_prompt("Description")
        .interact_text()?;

    let element: AppElement = AppElement::new(None, title, description, None);
    println!("\nYou are about to create the following new element:\n\n{}\n", element);
    if Confirm::new().with_prompt("Do you want to create this element?").interact()? {
        state.push(Some(element));
        state.unsynced();
        return Ok(());
    } else {
        return Ok(());
    }
}

/// Remove Dialog
fn remove_menu(state: &mut AppState) -> Result<(), io::Error> {
    state.list();
    println!("Select the ID of the element to be deleted:");

    let mut ids: Vec<String> = state.get_ids(true).into_iter().map(|e| e.to_string()).collect();
    ids.push("Exit".to_string());
    let last_element = ids.len() - 1;
    let selection: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("#")
        .items(&ids)
        .default(last_element)
        .interact_on_opt(&Term::stderr())?.unwrap_or(0);

    if selection == last_element {
        return Ok(());
    } else {
        let Ok(num) = ids[selection].parse::<u16>() else { return Ok(()) };
        if state.remove(num) {
            state.unsynced();
        };
    }

    Ok(())
}

/// Help Dialog (more a print but who cares)
fn help_menu() {
    println!("This is the Freemind Command Line Client");
    println!("You can perform different actions on your calendar and sync them");
    println!("with the Freemind API");
}

/// Main Dialog
async fn main_menu(config: AppConfig) -> Result<(), io::Error> {
    let mut last_index: usize = 0;
    let mut state = AppState::new(config);
    let commands: Vec<AppCommand> = AppCommand::get_command_list();
    loop {
        println!("================================");
        let selction: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt(">")
            .items(&commands)
            .default(last_index)
            .interact_on_opt(&Term::stderr())?.unwrap_or(0);
    
        println!("================================");
        last_index = selction;
        match AppCommand::from(selction) {
                AppCommand::List => state.list(),
                AppCommand::Sync => state.sync().await.unwrap(),
                AppCommand::Filter => filter_menu(),
                AppCommand::Edit => edit_menu(),
                AppCommand::Add => add_menu(&mut state)?,
                AppCommand::Remove => remove_menu(&mut state)?,
                AppCommand::Help => help_menu(),
                AppCommand::Quit => break,
            }
    }
    if !state.is_synced() {
        if Confirm::new().with_prompt("Attention: The current state seems to be unsynced with the server! Do you want to sync now?").interact()? {
            println!("Syncing...");
            state.sync().await.unwrap_or(());
        } else {
            println!("Discarding Changes...");
        }
    }
    println!("Bye!");
    Ok(())
}

#[tokio::main]
async fn main() {
    let args: ArgMatches = Command::new("Freemind CLI")
        .author(crate_authors!("\n"))
        .about(crate_description!())
        .version(crate_version!())
        .args_override_self(true)
        .arg(Arg::new("config")
            .short('c')
            .long("config")
            .action(ArgAction::SetTrue)
            .help("Enter the configuration setup")
        )
        .arg(Arg::new("skip-config-load")
            .long("skip-config-load")
            .action(ArgAction::SetTrue)
            .help("Skip loading and saving of the configuration file")
        )
        .get_matches();

    let config_setup: &bool = args.get_one("config").unwrap_or(&false);
    let config_skip: &bool = args.get_one("skip-config-load").unwrap_or(&false);

    let mut config = AppConfig::empty();
    if !config_skip {
        config = obtain_app_config()
            .expect("FATAL! Failed to create or read config! (tried under '~/.config/freemind/freemind-cli.config')\nRun with `--skip-config-load` to avoid this issue, or fix your file permissions!");
    }

    if *config_setup || config.is_default() || config.is_empty() {
        println!("Config could not be read, found or was skipped.\nEntering Configuration Setup:");
        config = setup_config(&config).expect("FATAL! Setup Dailog encountered an error!");
        if write_app_config(&config).is_none() {
            println!("ATTENTION: Config could not be written! Proceeding with supplied config this time...");
        } else {
            println!("Success!\n");
        }
    }

    // Config is now initialized! Now Deal with it.
    
    main_menu(config).await.expect("FATAL! Dialog encountered an error!");


}
