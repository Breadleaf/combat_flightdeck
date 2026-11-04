use std::{env, path};

mod config;

fn get_filepath_from_env() -> String {
    env::args()
        .nth(1)
        .unwrap_or_else(|| "./example.ron".to_string())
}

fn validate_filepath(path: &str) -> Result<(), String> {
    let p = path::Path::new(path);
    if path.trim().is_empty() {
        return Err("File path cannot be empty".into());
    }
    if !p.exists() {
        return Err(format!("File path: '{path}' does not exist"));
    }
    if !p.is_file() {
        return Err(format!(
            "File path: '{path}' is not a valid file, (might be a dir)"
        ));
    }
    Ok(())
}

fn validate_config(config: &config::Config) -> Result<(), String> {
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = get_filepath_from_env();
    validate_filepath(&path)?;
    let config = config::load_config(&path)?;
    validate_config(&config)?;
    println!("Successfully loaded config version {:?}!", config.version);
    println!("{:#?}", config);
    Ok(())
}
