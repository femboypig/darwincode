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

    if app.ui.setup.is_editing {
        if app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::Cancel, key)
        {
            app.ui.setup.is_editing = false;
            return Ok(());
        }
        if key.code == KeyCode::Enter {
            app.ui.setup.is_editing = false;
            return Ok(());
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::Delete, _) => {
                match app.ui.setup.active_field {
                    SetupField::ApiKey => app.ui.setup.api_key.clear(),
                    SetupField::Model => app.ui.setup.model.clear(),
                    SetupField::BaseUrl => app.ui.setup.base_url.clear(),
                    _ => {}
                }
            }
            (KeyCode::Backspace, _) => app.ui.setup.pop_char(),
            (KeyCode::Char(value), modifiers)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                let old_key = app.ui.setup.api_key.clone();
                app.ui.setup.push_char(value);
                if app.ui.setup.active_field == SetupField::ApiKey
                    && app.ui.setup.api_key.starts_with("sk-")
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
            if app.ui.setup.api_key.starts_with("sk-") {
                app.ui.setup.base_url = "http://localhost:20128/v1".to_owned();
                app.ui.setup.model = "claude-sonnet-4.6".to_owned();
                app.status = "Applied OmniRoute defaults for OpenAI/OmniRoute key".to_owned();
            }
        }
        (KeyCode::Up, _) | (KeyCode::BackTab, _) => {
            let current_idx = app.ui.setup.active_field.index();
            let next_idx = (current_idx + 9) % 10;
            app.ui.setup.active_field = SetupField::from_index(next_idx);
        }
        (KeyCode::Down, _) | (KeyCode::Tab, _) => {
            let current_idx = app.ui.setup.active_field.index();
            let next_idx = (current_idx + 1) % 10;
            app.ui.setup.active_field = SetupField::from_index(next_idx);
        }
        (KeyCode::Left, _) => match app.ui.setup.active_field {
            SetupField::Model => {
                app.ui.setup.select_previous_model();
            }
            SetupField::PermissionLevel => {
                app.ui.setup.permission_level = match app.ui.setup.permission_level {
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
                app.ui.setup.theme = app.ui.setup.theme.next();
            }
            _ => {}
        },
        (KeyCode::Right, _) => match app.ui.setup.active_field {
            SetupField::Model => {
                app.ui.setup.select_next_model();
            }
            SetupField::PermissionLevel => {
                app.ui.setup.permission_level = match app.ui.setup.permission_level {
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
                app.ui.setup.theme = app.ui.setup.theme.next();
            }
            _ => {}
        },
        (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => match app.ui.setup.active_field {
            SetupField::Save => {
                if let Err(error) = app.save_setup() {
                    app.status = error.to_string();
                }
            }
            SetupField::EnableCodebase => {
                app.ui.setup.enable_codebase_tools = !app.ui.setup.enable_codebase_tools;
            }
            SetupField::EnableBash => {
                app.ui.setup.enable_bash_tools = !app.ui.setup.enable_bash_tools;
            }
            SetupField::PermissionLevel => {
                app.ui.setup.permission_level = match app.ui.setup.permission_level {
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
                app.ui.setup.show_thoughts = !app.ui.setup.show_thoughts;
            }
            SetupField::Theme => {
                app.ui.setup.theme = app.ui.setup.theme.next();
            }
            SetupField::RespectIgnoreRules => {
                app.ui.setup.respect_ignore_rules = !app.ui.setup.respect_ignore_rules;
            }
            SetupField::ApiKey | SetupField::Model | SetupField::BaseUrl => {
                app.ui.setup.is_editing = true;
            }
        },
        (KeyCode::Char(value), modifiers)
            if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
        {
            match app.ui.setup.active_field {
                SetupField::ApiKey | SetupField::Model | SetupField::BaseUrl => {
                    app.ui.setup.is_editing = true;
                    let old_key = app.ui.setup.api_key.clone();
                    app.ui.setup.push_char(value);
                    if app.ui.setup.active_field == SetupField::ApiKey
                        && app.ui.setup.api_key.starts_with("sk-")
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
