use confy;
use data_types::{AppConfig, AppCommand};
use std::fs;
use std::io;
use clap::{Arg, Command, ArgMatches, crate_authors, crate_description, crate_version, ArgAction};
use dialoguer::{Input, Confirm, Password, FuzzySelect, Select, theme::ColorfulTheme, console::Term};

use crate::data_types::AuthMethod;

pub(crate) mod data_types {
    use std::fmt;
    use serde::{Serialize, Deserialize};
    

    //#[derive(Debug, EnumIter)]
    pub enum AppCommand {
        List,
        Sync,
        Filter,
        Add,
        Remove,
        Help,
        Quit,
    }

    impl ToString for AppCommand {
        fn to_string(&self) -> String {
            match self {
                List => "list",
                Sync => "sync",
                Filter => "filter",
                Add => "add",
                Remove => "remove",
                Help => "help",
                Quit => "quit",
            }.to_string()
        }
    }

    impl From<usize> for AppCommand {
        fn from(s: usize) -> Self {
            match s {
                0 => Self::List,
                1 => Self::Sync,
                2 => Self::Filter,
                3 => Self::Add,
                4 => Self::Remove,
                5 => Self::Help,
                6 => Self::Quit,
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

fn obtain_app_config() -> Option<AppConfig> {
    fs::create_dir_all("~/.config/freemind").ok();
    confy::load_path("~/.config/freemind/freemind-cli.config").ok()
}

fn write_app_config(config: &AppConfig) -> Option<()> {
    fs::create_dir_all("~/.config/freemind").ok();
    confy::store_path("~/.config/freemind/freemind-cli.config", config).ok();
    Some(())
}

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

fn main_menu(config: AppConfig) -> Result<(), io::Error> {
    let commands: Vec<AppCommand> = AppCommand::get_command_list();
    let selction: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
        .items(&commands)
        .default(0)
        .interact_on_opt(&Term::stderr())?.unwrap_or(0);

    let selected_command: AppCommand = AppCommand::from(selction);
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
