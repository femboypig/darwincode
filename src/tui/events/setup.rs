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
        .core.keybindings
        .matches(crate::tui::keybindings::TuiAction::Quit, key)
    {
        app.should_quit = true;
        return Ok(());
    }

    if app.ui.setup.is_editing {
        if app
            .core.keybindings
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
        .core.keybindings
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StoredConfig;
    use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};
    use std::sync::mpsc;

    #[test]
    fn test_handle_setup_key_navigation() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let mut app = App::new(Some(StoredConfig::default()));
        let (sender, _receiver) = mpsc::channel();
        app.ui.setup.active_field = SetupField::ApiKey;

        // Press Down arrow
        let key_down = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        handle_setup_key(&mut app, &sender, key_down).unwrap();
        assert_eq!(app.ui.setup.active_field, SetupField::Model);

        // Press Up arrow
        let key_up = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        handle_setup_key(&mut app, &sender, key_up).unwrap();
        assert_eq!(app.ui.setup.active_field, SetupField::ApiKey);
    }

    #[test]
    fn test_handle_setup_key_editing() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let mut app = App::new(Some(StoredConfig::default()));
        let (sender, _receiver) = mpsc::channel();
        app.ui.setup.active_field = SetupField::ApiKey;
        app.ui.setup.api_key.clear();

        // Type 'k'
        handle_setup_key(&mut app, &sender, KeyEvent::new(KeyCode::Char('k'), KeyModifiers::empty())).unwrap();
        assert!(app.ui.setup.is_editing);
        assert_eq!(app.ui.setup.api_key, "k");
    }

    #[test]
    fn test_handle_setup_key_comprehensive() {
        let _lock = crate::config::TEST_LOCK.lock().unwrap();
        let mut app = App::new(Some(StoredConfig::default()));
        let (sender, _receiver) = mpsc::channel();

        // 1. TuiAction::Quit
        app.should_quit = false;
        let key_quit = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        handle_setup_key(&mut app, &sender, key_quit).unwrap();
        assert!(app.should_quit);

        // Reset should_quit
        app.should_quit = false;

        // 2. Cancel editing
        app.ui.setup.is_editing = true;
        app.ui.setup.active_field = SetupField::ApiKey;
        let key_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        handle_setup_key(&mut app, &sender, key_esc).unwrap();
        assert!(!app.ui.setup.is_editing);

        // 3. Enter key in editing mode
        app.ui.setup.is_editing = true;
        let key_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        handle_setup_key(&mut app, &sender, key_enter).unwrap();
        assert!(!app.ui.setup.is_editing);

        // 4. Ctrl+U to clear input
        app.ui.setup.is_editing = true;
        app.ui.setup.api_key = "abc".to_owned();
        let key_ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        handle_setup_key(&mut app, &sender, key_ctrl_u).unwrap();
        assert!(app.ui.setup.api_key.is_empty());

        // 5. Backspace in editing mode
        app.ui.setup.api_key = "abc".to_owned();
        let key_bs = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
        handle_setup_key(&mut app, &sender, key_bs).unwrap();
        assert_eq!(app.ui.setup.api_key, "ab");

        // 6. Right arrow for PermissionLevel
        app.ui.setup.is_editing = false;
        app.ui.setup.active_field = SetupField::PermissionLevel;
        app.ui.setup.permission_level = crate::config::PermissionLevel::Safe;
        let key_right = KeyEvent::new(KeyCode::Right, KeyModifiers::empty());
        handle_setup_key(&mut app, &sender, key_right).unwrap();
        assert_eq!(app.ui.setup.permission_level, crate::config::PermissionLevel::Guardian);

        // Left arrow for PermissionLevel
        let key_left = KeyEvent::new(KeyCode::Left, KeyModifiers::empty());
        handle_setup_key(&mut app, &sender, key_left).unwrap();
        assert_eq!(app.ui.setup.permission_level, crate::config::PermissionLevel::Safe);

        // 7. Enter/Space to toggle boolean fields
        app.ui.setup.active_field = SetupField::EnableCodebase;
        app.ui.setup.enable_codebase_tools = false;
        handle_setup_key(&mut app, &sender, key_enter).unwrap();
        assert!(app.ui.setup.enable_codebase_tools);

        // RespectIgnoreRules toggle
        app.ui.setup.active_field = SetupField::RespectIgnoreRules;
        app.ui.setup.respect_ignore_rules = false;
        handle_setup_key(&mut app, &sender, key_enter).unwrap();
        assert!(app.ui.setup.respect_ignore_rules);

        // ShowThoughts toggle
        app.ui.setup.active_field = SetupField::ShowThoughts;
        app.ui.setup.show_thoughts = false;
        handle_setup_key(&mut app, &sender, key_enter).unwrap();
        assert!(app.ui.setup.show_thoughts);

        // EnableBash toggle
        app.ui.setup.active_field = SetupField::EnableBash;
        app.ui.setup.enable_bash_tools = false;
        handle_setup_key(&mut app, &sender, key_enter).unwrap();
        assert!(app.ui.setup.enable_bash_tools);

        // 8. Ctrl+A to apply OmniRoute defaults
        app.ui.setup.api_key = "sk-test".to_owned();
        app.ui.setup.base_url.clear();
        let key_ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        handle_setup_key(&mut app, &sender, key_ctrl_a).unwrap();
        assert_eq!(app.ui.setup.base_url, "http://localhost:20128/v1");
        assert_eq!(app.ui.setup.model, "claude-sonnet-4.6");
    }
}
