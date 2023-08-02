mod data;
use crate::data::data_types::{AppState, AppConfig, AppCommand, AppElement, AuthMethod};

#[macro_use] extern crate prettytable;
use confy;
use std::env;
use std::fs;
use std::io;
use std::ops::{Add, Sub};
use std::path::PathBuf;
use chrono::{TimeZone, Utc, LocalResult};
use clap::{Arg, Command, ArgMatches, crate_authors, crate_description, crate_version, ArgAction};
use dialoguer::{Input, Confirm, Password, FuzzySelect, Select, theme::ColorfulTheme, console::Term};
use prettytable::Table;



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

/// Helper with chrono that creates a timestamp that is *days* in the future
fn chrono_date_helper(days: i64) -> Option<u32> {
    let now = chrono::offset::Local::now();
    u32::try_from(if days != 0 {
        let tmrw = if let Ok(ut) = days.try_into() {
            now.add(chrono::naive::Days::new(ut))
        } else {
            now.sub(chrono::naive::Days::new((-1*days).try_into().unwrap_or(0)))
        };
        chrono::DateTime::from_local(
            chrono::naive::NaiveDateTime::new(
                tmrw.date_naive(),
                chrono::naive::NaiveTime::from_hms_opt(23, 59, 59).unwrap()),
            now.offset().to_owned())
    } else {
        now
    }.naive_utc().and_utc().timestamp()).ok()
}

/// Questions the user to input a datetime and returns the unix timestamp
fn get_datetime_from_user() -> Result<Option<u32>, std::io::Error> {
    let entered_input: String = Input::new()
                .with_prompt("Enter a number of days (e.g. '+1', '-1') or a full date with time (e.g. '04.06.23 19:00')")
                .validate_with(|input: &String| {
                    if input.starts_with("+") {
                        input[1..].parse::<i64>().is_ok()
                    } else if input.starts_with("-") {
                        input[0..].parse::<i64>().is_ok()
                    } else {
                        chrono::naive::NaiveDateTime::parse_from_str(input, "%d.%m.%y %H:%M").is_ok()
                    }.then_some(()).ok_or("Invalid format")
                })
                .interact_text()?;

            if entered_input.starts_with("+") {
                Ok(chrono_date_helper(entered_input[1..].parse::<i64>().unwrap_or(0)))
            } else if entered_input.starts_with("-") {
                Ok(chrono_date_helper(entered_input[0..].parse::<i64>().unwrap_or(0)))
            } else {
                let offset: String = chrono::Local::now().format("%z").to_string();
                Ok(u32::try_from(
                    chrono::DateTime::parse_from_str(
                        &format!("{} {}", entered_input, offset),"%d.%m.%y %H:%M %z"
                    )
                    .unwrap()
                    .naive_utc()
                    .timestamp()
                ).ok())
            }
}

fn get_element_id_from_user(state: &AppState) -> Result<Option<u16>, std::io::Error> {
    let mut ids: Vec<String> = state
        .get_ids(true)
        .into_iter()
        .map(|e| e.to_string())
        .collect();
    ids.push("Exit".to_string());
    let last_element = ids.len() - 1;
    let selection: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("#")
        .items(&ids)
        .default(last_element)
        .interact_on_opt(&Term::stderr())?.unwrap_or(0);

    if selection == last_element {
        Ok(None)
    } else {
        Ok(Some(ids[selection].parse::<u16>().unwrap()))
    }
}

fn filter_menu(state: &mut AppState) -> Result<(), std::io::Error> {
    let selection: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Filter according to")
        .items(&["due", "keyword"])
        .default(0)
        .interact_on_opt(&Term::stderr())?.unwrap_or(0);

    let mut table = Table::new();
    table.set_titles(row!["ID", "Title", "Description", "Due"]);

    match selection {
        0 => { // due
            let due_selection: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Filter due")
                .items(&["over", "the next day", "upcoming week", "next 4 weeks", "custom", "range"])
                .default(0)
                .interact_on_opt(&Term::stderr())?.unwrap_or(0);
            let mut timestamp_start: u32 = chrono_date_helper(-1).unwrap(); // Last day 23:59
            let timestamp_end: u32;
            match due_selection {
                0 => { // over
                    timestamp_start = 0;
                    timestamp_end = chrono_date_helper(0).unwrap();
                }
                1 => { // the next day
                    timestamp_end = chrono_date_helper(1).unwrap();
                },
                2 => { // upcoming week
                    timestamp_end = chrono_date_helper(7).unwrap();
                },
                3 => { // next 4 weeks
                    timestamp_end = chrono_date_helper(28).unwrap();
                },
                4 => { // custom
                    let timestamp_temp = get_datetime_from_user()?.unwrap_or(u32::MAX);
                    if timestamp_temp < timestamp_start {
                        timestamp_end = timestamp_start;
                        timestamp_start = timestamp_temp;
                    } else {
                        timestamp_end = timestamp_temp;
                    }
                },
                5 => { // range
                    println!("Set lower limit");
                    timestamp_start = get_datetime_from_user()?.unwrap_or(u32::MAX);
                    println!("Set upper limit");
                    timestamp_end = get_datetime_from_user()?.unwrap_or(u32::MAX);
                }
                _ => {return Ok(())},
            };

            state
                .get_elements()
                .iter()
                .filter(|e| {
                    let timestamp_element = e.get_timestamp().unwrap_or(u32::MAX);
                    timestamp_element > timestamp_start && timestamp_element < timestamp_end
                })
                .for_each(|e| {
                    table.add_row(e.to_row());
                });
        },
        1 => { // keyword
            let custom_filter: String = Input::<String>::new()
                .with_prompt("Keyword")
                .interact_text()?
                .to_lowercase();

            state
                .get_elements()
                .iter()
                .filter(|e| {e.get_text().contains(&custom_filter)})
                .for_each(|e| {
                    table.add_row(e.to_row());
                });

            },
        _ => ()
    };
    table.printstd();
        
    Ok(())
}

fn edit_menu(state: &mut AppState) -> Result<(), std::io::Error> {
    state.list();
    println!("Select the ID of the element to be edited:");

    let id: Option<u16> = get_element_id_from_user(state)?;
    
    match id {
        None => {return Ok(())},
        _ => ()
    };
    
    let element = state.get_element_by_id(id.unwrap());

    match element {
        None => {return Ok(())},
        _ => ()
    }

    let element = element.unwrap();

    let disp_due: String = match element.due() {
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

    let title: String = Input::new()
        .with_prompt("Title")
        .with_initial_text(element.title())
        .allow_empty(true)
        .interact_text()?;

    let description: String = Input::new()
        .with_prompt("Description")
        .with_initial_text(element.description())
        .allow_empty(true)
        .interact_text()?;

    let selection_due = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Due Date")
        .items(&[disp_due.as_ref(), "none", "tomorrow", "upcoming week", "custom"])
        .default(0)
        .interact_on_opt(&Term::stderr())?.unwrap_or(0);

    let due: Option<u32> = match selection_due {
        0 => {
            element.due()
        }, // Keep
        1 => None, // None
        2  => { // Tomorrow
            chrono_date_helper(1)
        },
        3 => { // Next Week
            chrono_date_helper(7)
        },
        4 => { // Custom
            get_datetime_from_user()?
        },
        _ => None,
    };

    let tags: Vec<String> = Input::<String>::new()
        .with_prompt("Enter Tags seperated by spaces (or leave empty)")
        .allow_empty(true)
        .with_initial_text(element.tags().join(" "))
        .interact_text()?
        .split(" ")
        .map(|e| e.to_owned())
        .collect::<Vec<String>>();

    let new_element: AppElement = AppElement::new(id, title, description, due, tags);
    println!("\nYou are about to change the element to the following values:\n\n{}\n", new_element);
    if Confirm::new().with_prompt("Do you want to apply these changes?").interact()? {
        element.modify(
            new_element.title(),
            new_element.description(),
            new_element.due(),
            new_element.tags()
        );
        state.unsynced();
        return Ok(());
    } else {
        return Ok(());
    }
}

/// Add Dialog
fn add_menu(state: &mut AppState) -> Result<(), std::io::Error> {
    let title: String = Input::new()
        .with_prompt("Title")
        .allow_empty(true)
        .interact_text()?;

    let description: String = Input::new()
        .with_prompt("Description")
        .allow_empty(true)
        .interact_text()?;

    let selection_due = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Due Date")
        .items(&["none", "tomorrow", "upcoming week", "custom"])
        .default(0)
        .interact_on_opt(&Term::stderr())?.unwrap_or(0);

    let due: Option<u32> = match selection_due {
        0 => None, // None
        1  => { // Tomorrow
            chrono_date_helper(1)
        },
        2 => { // Next Week
            chrono_date_helper(7)
        },
        3 => { // Custom
            get_datetime_from_user()?
        },
        _ => None,
    };

    let tags: Vec<String> = Input::<String>::new()
        .with_prompt("Enter Tags seperated by spaces (or leave empty)")
        .allow_empty(true)
        .interact_text()?
        .split(" ")
        .map(|e| e.to_owned())
        .collect::<Vec<String>>();

    let element: AppElement = AppElement::new(None, title, description, due, tags);
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

    match get_element_id_from_user(state)? {
        Some(id) => {
            if state.remove(id) {
                state.unsynced();
            };
        },
        None => {}
    };

    Ok(())
}

async fn boiling_menu(state: &mut AppState) -> Result<(), io::Error> {
    println!("Entering Boiling Mode...");
    println!("All chnages are live now!");

    let commands = ["get by id", "exit"];
    let mut last_index: usize = commands.len() - 1;

    loop {
        let (width, _height) = termion::terminal_size().unwrap_or((60, 60));
        println!("{}", "=".repeat(width as usize));
        let selection: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt("@>")
            .items(&commands)
            .default(last_index)
            .interact_on_opt(&Term::stderr())?.unwrap_or(0);
        println!("{}", "=".repeat(width as usize));
        last_index = selection;
        match selection {
            0 => {
                let input_id: String = Input::new()
                    .with_prompt("ID")
                    .validate_with(|input: &String| {
                        input
                            .parse::<u16>()
                            .is_ok()
                            .then_some(())
                            .ok_or("Invalid format")
                    })
                    .interact_text()?;
                let output: String = state.live_get_by_id(
                    input_id.parse::<u16>().unwrap()
                ).await.unwrap_or("Network Communication Error!".to_string()); // TODO: FIXME: unwrap is inappropriate this may fail
                println!("{}", output);
            },
            1 => {break},
            _ => (),
        }
    }
    println!("Returning to Normal Mode...");
    Ok(())

}

/// Help Dialog (more a print but who cares)
fn help_menu() {
    println!("This is the Freemind Command Line Client");
    println!("You can perform different actions on your calendar");
    println!("Normally all changes you make are Local until you");
    println!("explicitly sync them.");
    println!("An exception to this is the boiling mode.");
    println!("In boiling mode all operations are performed");
    println!("live on the Server!");
}

/// Main Dialog
async fn main_menu(config: AppConfig) -> Result<(), io::Error> {
    let mut last_index: usize = 0;
    let mut state: AppState = AppState::new(config);
    let commands: Vec<AppCommand> = AppCommand::get_command_list();
    
    loop {
        let (width, _height) = termion::terminal_size().unwrap_or((60, 60));
        
        println!("{}", "=".repeat(width as usize));
        let selection: usize = FuzzySelect::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("{}>", state.modified_string()))
            .items(&commands)
            .default(last_index)
            .interact_on_opt(&Term::stderr())?.unwrap_or(0);
        println!("{}", "=".repeat(width as usize));

        last_index = selection;
        match AppCommand::from(selection) {
                AppCommand::List => state.list(),
                AppCommand::Sync => state.sync().await.unwrap(),
                AppCommand::Filter => filter_menu(&mut state)?,
                AppCommand::Edit => edit_menu(&mut state)?,
                AppCommand::Add => add_menu(&mut state)?,
                AppCommand::Remove => remove_menu(&mut state)?,
                AppCommand::Boiling => boiling_menu(&mut state).await?,
                AppCommand::Help => help_menu(),
                AppCommand::Quit => break,
                _ => {println!("Not yet implemented")}
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
