use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::mpsc::Sender;

use crate::app::{App, SetupField};
use crate::tui::WorkerEvent;

pub(crate) fn handle_setup_key(
    app: &mut App,
    _sender: &Sender<WorkerEvent>,
    key: KeyEvent,
) -> Result<()> {
    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Quit, key)
    {
        app.should_quit = true;
        return Ok(());
    }

    if app.setup.is_editing {
        if app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::Cancel, key)
        {
            app.setup.is_editing = false;
            return Ok(());
        }
        if key.code == KeyCode::Enter {
            app.setup.is_editing = false;
            return Ok(());
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::Delete, _) => {
                match app.setup.active_field {
                    SetupField::ApiKey => app.setup.api_key.clear(),
                    SetupField::Model => app.setup.model.clear(),
                    SetupField::BaseUrl => app.setup.base_url.clear(),
                    _ => {}
                }
            }
            (KeyCode::Backspace, _) => app.setup.pop_char(),
            (KeyCode::Char(value), modifiers)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                let old_key = app.setup.api_key.clone();
                app.setup.push_char(value);
                if app.setup.active_field == SetupField::ApiKey
                    && app.setup.api_key.starts_with("sk-")
                    && !old_key.starts_with("sk-")
                {
                    app.status =
                        "OpenAI key detected. Press Ctrl+A to apply OmniRoute defaults.".to_owned();
                }
            }
            _ => {}
        }
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        app.cancel_setup();
        return Ok(());
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
            if app.setup.api_key.starts_with("sk-") {
                app.setup.base_url = "http://localhost:20128/v1".to_owned();
                app.setup.model = "claude-sonnet-4.6".to_owned();
                app.status = "Applied OmniRoute defaults for OpenAI/OmniRoute key".to_owned();
            }
        }
        (KeyCode::Up, _) | (KeyCode::BackTab, _) => {
            let current_idx = app.setup.active_field.index();
            let next_idx = (current_idx + 9) % 10;
            app.setup.active_field = SetupField::from_index(next_idx);
        }
        (KeyCode::Down, _) | (KeyCode::Tab, _) => {
            let current_idx = app.setup.active_field.index();
            let next_idx = (current_idx + 1) % 10;
            app.setup.active_field = SetupField::from_index(next_idx);
        }
        (KeyCode::Left, _) => match app.setup.active_field {
            SetupField::Model => {
                app.setup.select_previous_model();
            }
            SetupField::PermissionLevel => {
                app.setup.permission_level = match app.setup.permission_level {
                    crate::config::PermissionLevel::Safe => crate::config::PermissionLevel::Chaos,
                    crate::config::PermissionLevel::Guardian => {
                        crate::config::PermissionLevel::Safe
                    }
                    crate::config::PermissionLevel::Chaos => {
                        crate::config::PermissionLevel::Guardian
                    }
                };
            }
            SetupField::Theme => {
                app.setup.theme = app.setup.theme.next();
            }
            _ => {}
        },
        (KeyCode::Right, _) => match app.setup.active_field {
            SetupField::Model => {
                app.setup.select_next_model();
            }
            SetupField::PermissionLevel => {
                app.setup.permission_level = match app.setup.permission_level {
                    crate::config::PermissionLevel::Safe => {
                        crate::config::PermissionLevel::Guardian
                    }
                    crate::config::PermissionLevel::Guardian => {
                        crate::config::PermissionLevel::Chaos
                    }
                    crate::config::PermissionLevel::Chaos => crate::config::PermissionLevel::Safe,
                };
            }
            SetupField::Theme => {
                app.setup.theme = app.setup.theme.next();
            }
            _ => {}
        },
        (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => match app.setup.active_field {
            SetupField::Save => {
                if let Err(error) = app.save_setup() {
                    app.status = error.to_string();
                }
            }
            SetupField::EnableCodebase => {
                app.setup.enable_codebase_tools = !app.setup.enable_codebase_tools;
            }
            SetupField::EnableBash => {
                app.setup.enable_bash_tools = !app.setup.enable_bash_tools;
            }
            SetupField::PermissionLevel => {
                app.setup.permission_level = match app.setup.permission_level {
                    crate::config::PermissionLevel::Safe => {
                        crate::config::PermissionLevel::Guardian
                    }
                    crate::config::PermissionLevel::Guardian => {
                        crate::config::PermissionLevel::Chaos
                    }
                    crate::config::PermissionLevel::Chaos => crate::config::PermissionLevel::Safe,
                };
            }
            SetupField::ShowThoughts => {
                app.setup.show_thoughts = !app.setup.show_thoughts;
            }
            SetupField::Theme => {
                app.setup.theme = app.setup.theme.next();
            }
            SetupField::RespectGitignore => {
                app.setup.respect_gitignore = !app.setup.respect_gitignore;
            }
            SetupField::ApiKey | SetupField::Model | SetupField::BaseUrl => {
                app.setup.is_editing = true;
            }
        },
        (KeyCode::Char(value), modifiers)
            if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
        {
            match app.setup.active_field {
                SetupField::ApiKey | SetupField::Model | SetupField::BaseUrl => {
                    app.setup.is_editing = true;
                    let old_key = app.setup.api_key.clone();
                    app.setup.push_char(value);
                    if app.setup.active_field == SetupField::ApiKey
                        && app.setup.api_key.starts_with("sk-")
                        && !old_key.starts_with("sk-")
                    {
                        app.status =
                            "OpenAI key detected. Press Ctrl+A to apply OmniRoute defaults."
                                .to_owned();
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }

    Ok(())
}
