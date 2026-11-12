use std::{env, path, process};

mod config;
mod hardware;
mod runtime;
mod validate;

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

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = get_filepath_from_env();
    validate_filepath(&path)?;
    let config = config::load_config(&path)?;
    let runtime = runtime::Runtime::new(&config)
        .map_err(|errs| format!("invalid config:\n {}", errs.join("\n ")))?;
    println!("successfully loaded the config {:?}!", config.version);
    hardware::run(&config, runtime)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
        process::exit(1);
    }
}
