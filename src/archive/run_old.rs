use chrono::prelude::*;
#[allow(unused, dead_code)]
use clap::{value_parser, Arg, ArgAction, Command};
use confy;
use rpassword;
use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::str::from_utf8;

use crate::config::Config;
use crate::encryption;
use crate::scan::Scan;
use crate::settings::ConfigFile;

pub fn run(config: Config) -> Result<(), Box<dyn Error>> {
    if config.reset_settings {
        println!("Clearing configs");
        confy::store("sift", None, ConfigFile::default())?;
    }

    let mut app_settings: ConfigFile = confy::load("sift", None)?;

    let key = "SIFTPW";
    let mut password = match env::var(key) {
        Ok(p) => {
            println!("INFO: Using password from env");
            p
        }
        Err(_) => String::new(),
    };

    if config.root.is_some() {
        println!("adding roots: {:?}", config.root);
        app_settings.initial_scan = true;
        for root in config.root.unwrap() {
            if !app_settings.roots.contains(&root) {
                app_settings.roots.push(root);
            } else {
                println!("!'{}' already in root list", root);
            }
        }
    }

    if config.remove_roots.is_some() {
        println!("removing roots: {:?}", config.remove_roots);
        for root_to_remove in config.remove_roots.unwrap() {
            for (index, root) in app_settings.roots.clone().iter().enumerate() {
                if &root_to_remove == root {
                    app_settings.roots.remove(index);
                }
            }
        }
    }

    if config.print_output_directory {
        println!(
            "output directory: {:?}",
            app_settings.output_directory.as_ref().unwrap()
        );
    }

    if config.display_keywords
        || config.scan
        || config.full_scan
        || config.remove_keywords.is_some()
        || config.add_keywords.is_some()
    {
        if password.is_empty() {
            password = match app_settings.secret {
                Some(_) => rpassword::prompt_password("Enter password: ")?,
                None => rpassword::prompt_password("Enter new password: ")?,
            };
        }

        let valid_password: bool = match app_settings.secret {
            None => {
                let hashed_password = encryption::hash_password(&password).unwrap();
                app_settings.secret = Some(hashed_password);
                true
            }
            _ => encryption::verify_password(&password, app_settings.secret.as_ref().unwrap())
                .unwrap(),
        };

        if !valid_password {
            eprintln!("\nInvalid password");
            std::process::exit(1);
        }
    }

    if config.add_keywords.is_some() {
        let keywords = load_keywords(&app_settings.keywords, &password).unwrap();
        println!("adding keywords: {:?}", config.add_keywords);
        app_settings.initial_scan = true;
        for word in config.add_keywords.unwrap() {
            if !keywords.contains(&word) {
                app_settings
                    .keywords
                    .push(encryption::encrypt(word.as_bytes(), &password));
                app_settings.initial_scan = true;
            }
        }
    }

    if config.remove_keywords.is_some() {
        let keywords = load_keywords(&app_settings.keywords, &password).unwrap();
        println!("removing keywords: {:?}", config.remove_keywords);
        for word in config.remove_keywords.unwrap() {
            for (index, keyword) in keywords.iter().enumerate() {
                if &word == keyword {
                    app_settings.keywords.remove(index);
                }
            }
        }
    }

    if config.output_directory.is_some() {
        println!(
            "changing output directory to: {:?}",
            config.output_directory
        );
        app_settings.initial_scan = true;
        app_settings.output_directory = config.output_directory;
    }

    confy::store("sift", None, &app_settings)?;

    if config.display_root {
        println!("_roots_");
        for (index, root) in app_settings.roots.iter().enumerate() {
            println!("{:<1}: {:>5}", index + 1, root);
        }
    }

    if config.print_settings {
        println!("{:#?}", app_settings);
        println!("{:?}", confy::get_configuration_file_path("sift", None));
    }

    let keywords = load_keywords(&app_settings.keywords, &password).unwrap();

    if config.display_keywords {
        println!("_keywords_");
        for (index, keyword) in keywords.iter().enumerate() {
            println!("{:<1}: {:>5}", index + 1, keyword);
        }
    }

    if !prescan_checks(&app_settings) {
        println!("!!!Pre-scan checks failed.!!!");
        return Ok(());
    }

    if config.scan || config.full_scan {
        if config.full_scan {
            app_settings.initial_scan = false;
            let scan = Scan::new(
                config.full_scan,
                config.verbose,
                keywords.clone(),
                app_settings.roots.clone(),
                None,
                PathBuf::from(&app_settings.output_directory.as_ref().unwrap()),
                config.case_sensitive,
            );
            app_settings.time_last_scan = scan.time_stamp.to_string();
        } else if app_settings.initial_scan {
            app_settings.initial_scan = false;
            let scan = Scan::new(
                true,
                config.verbose,
                keywords.clone(),
                app_settings.roots.clone(),
                None,
                PathBuf::from(&app_settings.output_directory.as_ref().unwrap()),
                config.case_sensitive,
            );
            app_settings.time_last_scan = scan.time_stamp.to_string();
        } else {
            let last_scan_time: DateTime<Utc> = app_settings.time_last_scan.parse().unwrap();
            let scan = Scan::new(
                false,
                config.verbose,
                keywords.clone(),
                app_settings.roots.clone(),
                Some(last_scan_time),
                PathBuf::from(&app_settings.output_directory.as_ref().unwrap()),
                config.case_sensitive,
            );
            app_settings.time_last_scan = scan.time_stamp.to_string();
        }
    }

    confy::store("sift", None, &app_settings)?;

    Ok(())
}

// HELPERS //
fn load_keywords(
    encrypted_keywords: &Vec<String>,
    password: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut decrypted_keywords: Vec<String> = Vec::new();
    for word in encrypted_keywords {
        let decrypted_bytes = encryption::decrypt(word.as_str(), password)?;
        let decrypted_word = from_utf8(&decrypted_bytes)?;
        decrypted_keywords.push(String::from(decrypted_word));
    }
    Ok(decrypted_keywords)
}

fn prescan_checks(app_settings: &ConfigFile) -> bool {
    let mut scan_status = true;
    if app_settings.output_directory.is_none() {
        println!("!Pre-scan check failed:: No output directory designated.");
        scan_status = false
    }
    if app_settings.keywords.is_empty() {
        println!("!Pre-scan check failed:: No keywords designated.");
        scan_status = false;
    }
    if app_settings.roots.is_empty() {
        println!("!Pre-scan check failed:: No root directories designated.");
        scan_status = false;
    }
    if app_settings.secret.is_none() {
        println!("!Pre-scan check failed:: No application secret stored");
        scan_status = false;
    }
    if scan_status {
        println!("Pre-scan checks passed");
    }
    scan_status
}
