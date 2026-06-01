use std::sync::mpsc::Sender;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Screen, SetupField, SubmitAction};
use crate::tui::{WorkerEvent, spawn_generation_worker, spawn_models_worker, handle_function_action};

pub(crate) fn handle_key(app: &mut App, sender: &Sender<WorkerEvent>, key: KeyEvent) -> Result<()> {
    match app.screen {
        Screen::Setup => handle_setup_key(app, sender, key),
        Screen::Chat => handle_chat_key(app, sender, key),
        Screen::Models => handle_models_key(app, key),
        Screen::Permissions => handle_permissions_key(app, key),
        Screen::Sessions => handle_sessions_key(app, key),
    }
}

pub(crate) fn handle_paste(app: &mut App, text: String) {
    if app.screen == Screen::Chat {
        if matches!(app.pending, Some(crate::app::PendingTask::ConfirmFunction { .. })) {
            return;
        }
        app.chat.insert_text(&text);
    } else if app.screen == Screen::Setup {
        let old_key = app.setup.api_key.clone();
        for c in text.chars() {
            app.setup.push_char(c);
        }
        if app.setup.api_key.starts_with("sk-") && !old_key.starts_with("sk-") {
            app.status = "OpenAI key detected. Press Ctrl+A to apply OmniRoute defaults.".to_owned();
        }
    }
}

fn handle_permissions_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
            app.cancel_permissions();
        }
        (KeyCode::Up, _) => app.permissions.select_previous(),
        (KeyCode::Down, _) => app.permissions.select_next(),
        (KeyCode::Enter, _) => app.apply_permission_level(),
        _ => {}
    }
    Ok(())
}

fn handle_setup_key(app: &mut App, _sender: &Sender<WorkerEvent>, key: KeyEvent) -> Result<()> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
            if app.setup.api_key.starts_with("sk-") {
                app.setup.base_url = "http://localhost:20128/v1".to_owned();
                app.setup.model = "claude-sonnet-4.6".to_owned();
                app.status = "Applied OmniRoute defaults for OpenAI/OmniRoute key".to_owned();
            }
        }
        (KeyCode::Esc, _) => app.cancel_setup(),
        (KeyCode::Tab, _) => app.setup.next_field(),
        (KeyCode::BackTab, _) => app.setup.previous_field(),
        (KeyCode::Enter, _) => match app.setup.active_field {
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
                    crate::config::PermissionLevel::Safe => crate::config::PermissionLevel::Guardian,
                    crate::config::PermissionLevel::Guardian => crate::config::PermissionLevel::Chaos,
                    crate::config::PermissionLevel::Chaos => crate::config::PermissionLevel::Safe,
                };
            }
            SetupField::ShowThoughts => {
                app.setup.show_thoughts = !app.setup.show_thoughts;
            }
            _ => app.setup.next_field(),
        },
        (KeyCode::Char(' '), modifiers)
            if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
        {
            match app.setup.active_field {
                SetupField::EnableCodebase => {
                    app.setup.enable_codebase_tools = !app.setup.enable_codebase_tools;
                }
                SetupField::EnableBash => {
                    app.setup.enable_bash_tools = !app.setup.enable_bash_tools;
                }
                SetupField::PermissionLevel => {
                    app.setup.permission_level = match app.setup.permission_level {
                        crate::config::PermissionLevel::Safe => crate::config::PermissionLevel::Guardian,
                        crate::config::PermissionLevel::Guardian => crate::config::PermissionLevel::Chaos,
                        crate::config::PermissionLevel::Chaos => crate::config::PermissionLevel::Safe,
                    };
                }
                SetupField::ShowThoughts => {
                    app.setup.show_thoughts = !app.setup.show_thoughts;
                }
                _ => app.setup.push_char(' '),
            }
        }
        (KeyCode::Backspace, _) => app.setup.pop_char(),
        (KeyCode::Up, _) if app.setup.active_field == SetupField::Model => {
            app.setup.select_previous_model();
        }
        (KeyCode::Down, _) if app.setup.active_field == SetupField::Model => {
            app.setup.select_next_model();
        }
        (KeyCode::Up, _) if app.setup.active_field == SetupField::PermissionLevel => {
            app.setup.permission_level = match app.setup.permission_level {
                crate::config::PermissionLevel::Safe => crate::config::PermissionLevel::Chaos,
                crate::config::PermissionLevel::Guardian => crate::config::PermissionLevel::Safe,
                crate::config::PermissionLevel::Chaos => crate::config::PermissionLevel::Guardian,
            };
        }
        (KeyCode::Down, _) if app.setup.active_field == SetupField::PermissionLevel => {
            app.setup.permission_level = match app.setup.permission_level {
                crate::config::PermissionLevel::Safe => crate::config::PermissionLevel::Guardian,
                crate::config::PermissionLevel::Guardian => crate::config::PermissionLevel::Chaos,
                crate::config::PermissionLevel::Chaos => crate::config::PermissionLevel::Safe,
            };
        }
        (KeyCode::Char(value), modifiers)
            if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
        {
            let old_key = app.setup.api_key.clone();
            app.setup.push_char(value);
            if app.setup.api_key.starts_with("sk-") && !old_key.starts_with("sk-") {
                app.status = "OpenAI key detected. Press Ctrl+A to apply OmniRoute defaults.".to_owned();
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_chat_key(app: &mut App, sender: &Sender<WorkerEvent>, key: KeyEvent) -> Result<()> {
    if matches!(app.pending, Some(crate::app::PendingTask::ConfirmFunction { .. })) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(action) = app.answer_function_confirmation(true) {
                    handle_function_action(action, sender);
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                if let Some(action) = app.answer_function_confirmation(false) {
                    handle_function_action(action, sender);
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            _ => {}
        }
        return Ok(());
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        (KeyCode::Esc, _) => {
            if matches!(app.pending, Some(crate::app::PendingTask::Generating)) {
                app.pending = None;
                app.status = "Generation stopped".to_owned();
                if app.chat.messages.last().is_some_and(|m| m.pending) {
                    app.chat.messages.pop();
                }
                app.chat.streaming_parts.clear();
                app.chat.message_queue.clear();
                let _ = crate::app::session::save_session(&app.chat);
            }
        }
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => app.open_setup(),
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
            if let Some(config) = app.begin_load_chat_models() {
                spawn_models_worker(config, sender.clone());
            }
        }
        (KeyCode::Tab, _) => app.accept_command_suggestion(),
        (KeyCode::Enter, modifiers) if !modifiers.is_empty() => {
            app.chat.insert_char('\n');
        }
        (KeyCode::Enter, _) => {
            if let Some(action) = app.submit_chat_input() {
                match action {
                    SubmitAction::Generate(request) => {
                        spawn_generation_worker(request.config, request.history, sender.clone());
                    }
                    SubmitAction::LoadModels(config) => {
                        spawn_models_worker(config, sender.clone());
                    }
                }
            }
        }
        (KeyCode::Backspace, _) => app.chat.remove_char(),
        (KeyCode::Delete, _) => app.chat.delete_char(),
        (KeyCode::Left, _) => app.chat.move_cursor_left(),
        (KeyCode::Right, _) => app.chat.move_cursor_right(),
        (KeyCode::Up, _) => {
            if app.chat.input.contains('\n') {
                let old_cursor = app.chat.cursor;
                app.chat.move_cursor_up();
                if app.chat.cursor == old_cursor {
                    app.chat.scroll = app.chat.scroll.saturating_add(1);
                }
            } else {
                app.chat.scroll = app.chat.scroll.saturating_add(1);
            }
        }
        (KeyCode::Down, _) => {
            if app.chat.input.contains('\n') {
                let old_cursor = app.chat.cursor;
                app.chat.move_cursor_down();
                if app.chat.cursor == old_cursor {
                    app.chat.scroll = app.chat.scroll.saturating_sub(1);
                }
            } else {
                app.chat.scroll = app.chat.scroll.saturating_sub(1);
            }
        }
        (KeyCode::Home, _) => app.chat.move_cursor_start(),
        (KeyCode::End, _) => app.chat.move_cursor_end(),
        (KeyCode::PageUp, _) => {
            app.chat.scroll = app.chat.scroll.saturating_add(5);
        }
        (KeyCode::PageDown, _) => {
            app.chat.scroll = app.chat.scroll.saturating_sub(5);
        }
        (KeyCode::Char(value), modifiers)
            if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
        {
            app.chat.insert_char(value);
        }
        _ => {}
    }

    Ok(())
}

fn handle_models_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => app.cancel_models(),
        (KeyCode::Up, _) => app.select_previous_model(),
        (KeyCode::Down, _) => app.select_next_model(),
        (KeyCode::Enter, _) => app.apply_selected_model(),
        _ => {}
    }

    Ok(())
}

fn handle_sessions_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
            app.cancel_sessions();
        }
        (KeyCode::Up, _) => app.sessions.select_previous(),
        (KeyCode::Down, _) => app.sessions.select_next(),
        (KeyCode::Enter, _) => app.apply_selected_session(),
        _ => {}
    }
    Ok(())
}
