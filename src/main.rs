mod api;
mod app;
mod config;
mod crypto;
mod tui;

use anyhow::Result;

use crate::app::App;
use crate::config::StoredConfig;

fn main() -> Result<()> {
    let args = std::env::args().skip(1);
    let mut continue_session = false;
    let mut show_version = false;
    let mut path_arg = None;

    for arg in args {
        match arg.as_str() {
            "-v" | "--version" => {
                show_version = true;
            }
            "-c" | "--continue" => {
                continue_session = true;
            }
            "-h" | "--help" => {
                println!(
                    "darwincode {} - The open source terminal AI coding agent",
                    env!("CARGO_PKG_VERSION")
                );
                println!();
                println!("Usage: darwincode [OPTIONS] [PATH]");
                println!();
                println!("Options:");
                println!("  -v, --version   Print version and exit");
                println!("  -c, --continue  Continue the last session");
                println!("  -h, --help      Print help");
                return Ok(());
            }
            s if s.starts_with('-') => {
                eprintln!("Error: Unknown option '{}'", s);
                std::process::exit(1);
            }
            path => {
                if path_arg.is_some() {
                    eprintln!("Error: Multiple path arguments provided");
                    std::process::exit(1);
                }
                path_arg = Some(path.to_owned());
            }
        }
    }

    if show_version {
        println!("darwincode {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Some(ref path) = path_arg {
        let resolved = resolve_path(path);
        if !resolved.exists() {
            eprintln!("Error: Path '{}' does not exist", path);
            std::process::exit(1);
        }
        std::env::set_current_dir(&resolved)?;
    }

    let config = StoredConfig::load()?;
    let mut app = App::new(config);

    if continue_session && let Ok(sessions) = crate::app::session::list_saved_sessions() {
        if let Some(latest) = sessions.first() {
            if let Err(e) = app.resume_session(&latest.id) {
                app.status = format!("Failed to load last session: {}. Started a new session.", e);
            }
        } else {
            app.status = "No saved sessions found. Started a new session.".to_owned();
        }
    }

    tui::run(app)
}

fn resolve_path(path: &str) -> std::path::PathBuf {
    if (path.starts_with("~/") || path == "~")
        && let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
    {
        if path == "~" {
            home
        } else {
            home.join(&path[2..])
        }
    } else {
        std::path::PathBuf::from(path)
    }
}
