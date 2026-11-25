use std::{env, path, process};

mod config;
mod hardware;
mod runtime;
mod setup;
mod validate;

fn get_filepath_from_env() -> String {
    env::args()
        .nth(1)
        .unwrap_or_else(|| "./example_setup.ron".to_string())
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
    if path == "--list" {
        return hardware::list_gamepads();
    }
    validate_filepath(&path)?;

    let loaded = setup::load_all(&path)?;

    let mut entries = Vec::new();
    let mut errs: Vec<String> = Vec::new();
    for r in &loaded {
        match runtime::Runtime::new(&r.config) {
            Ok(runtime) => entries.push(hardware::Entry {
                name: &r.name,
                controller: &r.controller,
                runtime,
            }),
            Err(es) => errs.extend(es.into_iter().map(|e| format!("robot '{}': {e}", r.name))),
        }
    }
    if !errs.is_empty() {
        return Err(format!("invalid config:\n {}", errs.join("\n ")).into());
    }

    setup::validate_event(&loaded)
        .map_err(|errs| format!("invalid setup:\n {}", errs.join("\n ")))?;

    println!("successfully loaded setup with {} robot(s)!", loaded.len());
    hardware::run(&loaded[0].config.link, entries)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
        process::exit(1);
    }
}
