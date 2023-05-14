#[macro_use] extern crate prettytable;
use confy;
use data_types::{AppState, AppConfig, AppCommand};
use std::fs;
use std::io;
use clap::{Arg, Command, ArgMatches, crate_authors, crate_description, crate_version, ArgAction};
use dialoguer::{Input, Confirm, Password, FuzzySelect, Select, theme::ColorfulTheme, console::Term};

use crate::data_types::AuthMethod;

pub(crate) mod data_types {
    use std::fmt;
    use chrono::{TimeZone, Utc, LocalResult};
    use prettytable::{Table, Row};
    use serde::{Serialize, Deserialize};
    
    #[derive(Serialize, Deserialize)]
    pub struct AppElement {
        id: Option<u16>,
        title: String,
        description: String,
        due: Option<u32>,
    }

    impl AppElement {
        fn to_row(&self) -> Row {
            let due_timestamp: i64 = self.due.unwrap_or(u32::MAX).into();
            let utc_due: String = match Utc.timestamp_opt(due_timestamp, 0) {
                LocalResult::None => "None".to_string(),
                LocalResult::Single(val) => val.to_rfc2822(),
                LocalResult::Ambiguous(val, _) => val.to_rfc2822(),
            };
            //let offset = Local::now().offset();
            row![
                format!("{:?}", self.id),
                self.title,
                self.description,
                utc_due,
            ]
        }
    }

    pub struct AppState {
        config: AppConfig,
        elements: Vec<AppElement>,
        synced: bool,
    }

    impl AppState {
        pub fn new(config: AppConfig) -> Self {
            Self {
                config,
                elements: Vec::new(),
                synced: false,
            }
        }
        pub fn list(&self) {
            let mut table = Table::new();
            table.set_titles(row!["ID", "Title", "Description", "Due"]);
            self.elements.iter().map(|e| table.add_row(e.to_row()));
        }

        pub fn sync(&self) {

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
        pub fn is_quit(&self) -> bool {
            &Self::Quit == self
        }
        pub fn get_command_list() -> Vec<AppCommand> {
            vec![
                Self::List,
                Self::Sync,
                Self::Filter,
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
        secret: String,
        auth_method: AuthMethod,
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
    fs::create_dir_all("~/.config/freemind").ok();
    confy::load_path("~/.config/freemind/freemind-cli.config").ok()
}

/// Save the app configuration
fn write_app_config(config: &AppConfig) -> Option<()> {
    fs::create_dir_all("~/.config/freemind").ok();
    confy::store_path("~/.config/freemind/freemind-cli.config", config).ok();
    Some(())
}

/// Configuration Setup Dialog
fn setup_config(prev_config: &AppConfig) -> Result<AppConfig, std::io::Error> {
    println!("\n   ### Config Setup: ###\n");
    let server_address: String = Input::new()
        .with_prompt("URL of the server to connect to https:// ")
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


fn filter_menu()

/// Help dialog (more a print but who cares)
fn help_menu() {
    println!("This is the Freemind Command Line Client");
    println!("You can perform different actions on your calendar and sync them");
    println!("with the Freemind API");
}

/// Main Dialog
fn main_menu(config: AppConfig) -> Result<(), io::Error> {
    let mut state = AppState::new(config);
    let commands: Vec<AppCommand> = AppCommand::get_command_list();
    loop {
        let selction: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose what you want to do")
            .items(&commands)
            .default(0)
            .interact_on_opt(&Term::stderr())?.unwrap_or(0);
    
        match AppCommand::from(selction) {
                AppCommand::List => state.list(),
                AppCommand::Sync => state.sync(),
                AppCommand::Filter => filter_menu(),
                AppCommand::Edit => edit_menu(),
                AppCommand::Add => add_menu(),
                AppCommand::Remove => remove_menu(),
                AppCommand::Help => help_menu(),
                AppCommand::Quit => break,
            }
    }
    Ok(())
}

fn main() {
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
    
    main_menu(config).expect("FATAL! Dialog encountered an error!");


}
