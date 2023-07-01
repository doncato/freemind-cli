pub(crate) mod data_types {
    use std::{fmt, io::Cursor, str};
    use chrono::{TimeZone, Utc, LocalResult};
    use serde::{Serialize, Deserialize};
    use reqwest::{Client, Response, header::HeaderValue};
    use prettytable::{Table, Row};
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

        /// A function that returns the title followed by the description
        /// as a single string, this is designed for usage of searching and
        /// filtering
        pub fn get_text(&self) -> String {
            return String::new() + &self.title + " " + &self.description;
        }

        /// Gets the timestamp
        pub fn get_timestamp(&self) -> Option<u32> {
            return self.due
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

        pub fn to_row(&self) -> Row {
            let disp_due: String = match self.due {
                Some(due) => {
                    let due_timestamp: i64 = due.into();
                    let utc_due: String = match Utc.timestamp_opt(due_timestamp, 0) {
                        LocalResult::None => "None".to_string(),
                        LocalResult::Single(val) => val.with_timezone(&chrono::Local).to_rfc2822(),
                        LocalResult::Ambiguous(val, _) => val.with_timezone(&chrono::Local).to_rfc2822(),
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

    /// The current state of the app
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

        /// Returns a string that supposes to indicate whether modifications
        /// have been made to the local state
        pub fn modified_string(&self) -> String {
            match self.synced {
                true => " ",
                false => "*",
            }.to_string()
        }


        /// Retreives the requested id directly from the server and returns a string
        /// with the content:
        pub async fn live_get_by_id(&mut self, id: u16) -> Result<String, reqwest::Error> {
            self.handle_empty_client();
            let mut result: String = String::new();
            
            let res: Response = self.call(&format!("/xml/get_by_id/{}", id), "".to_string()).await?;
            let headers = res.headers();
            
            if headers.get("content-type") != Some(&HeaderValue::from_static("text/xml")) {
                return Ok(result);
            }
            
            let xml: String = res.text().await?;

            let mut reader = Reader::from_str(&xml);
            
            reader.trim_text(true);

            let mut enabled: bool = false;
            let mut indentation: usize = 0;

            loop {
                match reader.read_event() {
                    Ok(Event::Start(e)) if e.name().as_ref() == b"entry" => {
                        // We expect just one entry here so it is fine to do it this way
                        enabled = true;
                    },
                    Ok(Event::End(e)) if e.name().as_ref() == b"entry" => {
                        enabled = false;
                    },
                    Ok(Event::Start(e)) if enabled => {
                        result.push_str(&" ".repeat(indentation));
                        result.push_str(str::from_utf8(e.name().as_ref()).unwrap_or(""));
                        result.push_str(": ");
                        indentation += 1;
                    }
                    Ok(Event::Text(txt)) if enabled => {
                        result.push_str(
                            &txt.unescape().unwrap()
                        );
                    }
                    Ok(Event::End(_)) if enabled => {
                        result.push_str("\n");
                        indentation -= 1;
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => ()
                }
            }

            Ok(result)
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
            let count_after: usize = self.elements
                .iter_mut()
                .filter(|e| e.id.is_none())
                .map(|e| {
                    new_ids.push(e.generate_id(existing_ids))
                }).count();
            (count_after != 0, new_ids)
        }

        /// Makes a call to the configured server using the provided endpoint
        async fn call(&mut self, endpoint: &str, payload: String) -> Result<Response, reqwest::Error> {
            self.handle_empty_client();
            let res: Response = self.client.as_ref().unwrap()
                .post(format!("{}{}", self.config.server_address, endpoint))
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

            Ok(res)
        }

        /// Fetches the whole registry from the server
        async fn fetch(&mut self) -> Result<String, reqwest::Error> {
            let res: Response = self.call("/xml/fetch", "".to_string()).await?;

            let headers = res.headers();
            if headers.get("content-type") == Some(&HeaderValue::from_static("text/xml")) {
                let txt = res.text().await?;
                return Ok(txt);
            }

            Ok(String::new())
        }

        /// Uploads the given payload to the server and returns the HTTP status code
        async fn upload(&mut self, payload: String) -> Result<u16, reqwest::Error> {
            let res: Response = self.call("/xml/update", payload).await?;

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
            let mut table: Table = Table::new();
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
        Boiling,
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
                Self::Boiling => "boiling",
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
                6 => Self::Boiling,
                7 => Self::Help,
                8 => Self::Quit,
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
                Self::Boiling,
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