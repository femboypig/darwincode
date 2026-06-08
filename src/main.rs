mod api;
mod app;
mod config;
mod crypto;
mod tui;

use anyhow::Result;

use crate::app::App;
use crate::config::StoredConfig;

fn main() -> Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableFocusChange,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show
        );
        default_hook(info);
    }));

    // Fail fast if the environment doesn't give us a writable
    // config directory. Without one we'd have to fall back to
    // writing the API key in cleartext, which is the M3 finding in
    // the project's security audit — see SECURITY.md.
    if crate::crypto::is_home_appdata_missing() {
        eprintln!(
            "darwincode: no usable config directory found.\n\
             Set one of $XDG_CONFIG_HOME, $HOME, $APPDATA, or $USERPROFILE\n\
             to a writable path before running darwincode."
        );
        std::process::exit(2);
    }

    // Automatically initialize the project-specific .darwincode directory structure
    crate::tui::theme::init_project_dir();

    let args_vec: Vec<String> = std::env::args().skip(1).collect();
    let mut continue_session = false;
    let mut show_version = false;
    let mut path_arg = None;
    let mut session_override = None;
    let mut model_override = None;
    let mut trust_workspace = false;

    let mut i = 0;
    while i < args_vec.len() {
        match args_vec[i].as_str() {
            "-v" | "--version" => {
                show_version = true;
                i += 1;
            }
            "-c" | "--continue" => {
                continue_session = true;
                i += 1;
            }
            "-s" | "--session" => {
                if i + 1 < args_vec.len() {
                    session_override = Some(args_vec[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Option '--session' requires a value");
                    std::process::exit(1);
                }
            }
            "-m" | "--model" => {
                if i + 1 < args_vec.len() {
                    model_override = Some(args_vec[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Option '--model' requires a value");
                    std::process::exit(1);
                }
            }
            "--trust-workspace" => {
                trust_workspace = true;
                i += 1;
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
                println!("  -v, --version           Print version and exit");
                println!("  -c, --continue          Continue the last session");
                println!("  -s, --session <ID>      Resume a specific session by ID");
                println!("  -m, --model <MODEL>     Use a specific model for this run");
                println!("  --trust-workspace       Trust workspace custom commands");
                println!("  -h, --help              Print help");
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
                i += 1;
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

    let mut config = StoredConfig::load()?;
    if let Some(ref mut cfg) = config {
        if let Some(model) = model_override {
            cfg.model = model;
        }
        if trust_workspace {
            cfg.trust_workspace = true;
        }
    } else if let Some(model) = model_override {
        let cfg = StoredConfig {
            model,
            trust_workspace,
            ..Default::default()
        };
        config = Some(cfg);
    } else if trust_workspace {
        let cfg = StoredConfig {
            trust_workspace,
            ..Default::default()
        };
        config = Some(cfg);
    }

    let mut app = App::new(config);

    if let Some(ref session_id) = session_override {
        if let Err(e) = app.resume_session(session_id) {
            app.status = format!(
                "Failed to load session '{}': {}. Started a new session.",
                session_id, e
            );
        }
    } else if continue_session && let Ok(sessions) = crate::app::session::list_saved_sessions() {
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
