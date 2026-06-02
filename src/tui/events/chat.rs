use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::mpsc::Sender;

use crate::app::{App, SubmitAction};
use crate::tui::{WorkerEvent, spawn_generation_worker, spawn_models_worker, handle_function_action};
use crate::tui::events::common::{copy_to_clipboard, read_from_clipboard};

pub(crate) fn handle_chat_key(app: &mut App, sender: &Sender<WorkerEvent>, key: KeyEvent) -> Result<()> {
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
            KeyCode::Up => {
                app.confirm_scroll = app.confirm_scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                app.confirm_scroll = app.confirm_scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                app.confirm_scroll = app.confirm_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                app.confirm_scroll = app.confirm_scroll.saturating_add(10);
            }
            _ => {
                if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollUp, key) {
                    app.confirm_scroll = app.confirm_scroll.saturating_sub(1);
                } else if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollDown, key) {
                    app.confirm_scroll = app.confirm_scroll.saturating_add(1);
                }
            }
        }
        return Ok(());
    }

    if app.chat.shell_focused {
        let is_ctrl_c = (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
            || app.keybindings.matches(crate::tui::keybindings::TuiAction::Quit, key);
        let is_esc = app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key);
        
        if is_ctrl_c || is_esc {
            if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
                if let Some(pid) = *guard {
                    #[cfg(unix)]
                    {
                        let _ = std::process::Command::new("kill")
                            .arg("-9")
                            .arg(format!("-{}", pid))
                            .status();
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = std::process::Command::new("taskkill")
                            .arg("/F")
                            .arg("/PID")
                            .arg(pid.to_string())
                            .status();
                    }
                    *guard = None;
                }
            }
            app.cancel_generation();
            app.chat.shell_focused = false;
            app.status = "Aborted by user".to_owned();
            return Ok(());
        }

        if key.code == KeyCode::Tab {
            app.chat.shell_focused = false;
            app.status = "Ready".to_owned();
            return Ok(());
        }

        if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollUp, key) {
            app.chat.scroll = app.chat.scroll.saturating_add(3);
            return Ok(());
        }
        if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollDown, key) {
            app.chat.scroll = app.chat.scroll.saturating_sub(3);
            return Ok(());
        }
        if app.keybindings.matches(crate::tui::keybindings::TuiAction::PageUp, key) {
            app.chat.scroll = app.chat.scroll.saturating_add(15);
            return Ok(());
        }
        if app.keybindings.matches(crate::tui::keybindings::TuiAction::PageDown, key) {
            app.chat.scroll = app.chat.scroll.saturating_sub(15);
            return Ok(());
        }

        if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_STDIN.lock() {
            if let Some(ref mut stdin) = *guard {
                use std::io::Write;
                let data = match key.code {
                    KeyCode::Char(c) => Some(c.to_string()),
                    KeyCode::Enter => Some("\n".to_owned()),
                    KeyCode::Backspace => Some("\x08".to_owned()),
                    _ => None,
                };
                if let Some(s) = data {
                    let _ = stdin.write_all(s.as_bytes());
                    let _ = stdin.flush();

                    if let Some(msg) = app.chat.messages.iter_mut().rev().find(|m| m.is_shell) {
                        if msg.text.ends_with("\nRunning...\n") {
                            msg.text.truncate(msg.text.len() - 11);
                        }
                        if key.code == KeyCode::Backspace {
                            if !msg.text.is_empty() {
                                msg.text.pop();
                            }
                        } else {
                            msg.text.push_str(&s);
                        }
                        *msg.cached_wrapped.borrow_mut() = None;
                    }
                }
            }
        }
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Quit, key) {
        app.should_quit = true;
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key) {
        if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
            if let Some(pid) = *guard {
                #[cfg(unix)]
                {
                    let _ = std::process::Command::new("kill")
                        .arg("-9")
                        .arg(format!("-{}", pid))
                        .status();
                }
                #[cfg(not(unix))]
                {
                    let _ = std::process::Command::new("taskkill")
                        .arg("/F")
                        .arg("/PID")
                        .arg(pid.to_string())
                        .status();
                }
                *guard = None;
            }
        }
        app.cancel_generation();
        app.status = "Stopped".to_owned();
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ToggleSetup, key) {
        app.open_setup();
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ToggleModels, key) {
        if let Some(config) = app.begin_load_chat_models() {
            spawn_models_worker(config, sender.clone());
        }
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ToggleSessions, key) {
        app.open_sessions();
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Submit, key) {
        if let Some(action) = app.submit_chat_input() {
            match action {
                SubmitAction::Generate(request) => {
                    spawn_generation_worker(request.config, request.history, request.cancel_token, request.generation_id, sender.clone());
                }
                SubmitAction::LoadModels(config) => {
                    spawn_models_worker(config, sender.clone());
                }
                SubmitAction::ExecuteFunction { name, args, config } => {
                    handle_function_action(crate::app::FunctionAction::Execute { name, args, config }, sender);
                }
            }
        }
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollUp, key) {
        if app.chat.input.contains('\n') {
            let old_cursor = app.chat.cursor;
            app.chat.move_cursor_up();
            if app.chat.cursor == old_cursor {
                app.chat.scroll = app.chat.scroll.saturating_add(3);
            }
        } else {
            app.chat.scroll = app.chat.scroll.saturating_add(3);
        }
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollDown, key) {
        if app.chat.input.contains('\n') {
            let old_cursor = app.chat.cursor;
            app.chat.move_cursor_down();
            if app.chat.cursor == old_cursor {
                app.chat.scroll = app.chat.scroll.saturating_sub(3);
            }
        } else {
            app.chat.scroll = app.chat.scroll.saturating_sub(3);
        }
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::PageUp, key) {
        app.chat.scroll = app.chat.scroll.saturating_add(15);
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::PageDown, key) {
        app.chat.scroll = app.chat.scroll.saturating_sub(15);
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::HistoryUp, key) {
        app.chat.navigate_history_up();
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::HistoryDown, key) {
        app.chat.navigate_history_down();
        return Ok(());
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('z'), KeyModifiers::CONTROL) => {
            app.chat.undo();
        }
        (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
            app.chat.redo();
        }
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
            let text = app.chat.input.clone();
            if !text.is_empty() {
                if copy_to_clipboard(&text).is_ok() {
                    app.status = "Copied input to clipboard".to_owned();
                }
                app.chat.save_history();
                app.chat.input.clear();
                app.chat.cursor = 0;
                app.chat.input_scroll = 0;
            }
        }
        (KeyCode::Char('y'), KeyModifiers::CONTROL) => {
            if let Ok(text) = read_from_clipboard() {
                app.chat.insert_text(&text);
            }
        }
        (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
            let mut last_response = None;
            for msg in app.chat.messages.iter().rev() {
                if msg.author == "Darwin" && !msg.is_tool && !msg.is_shell && !msg.pending {
                    let mut text = msg.text.trim();
                    if text.starts_with("(empty)") {
                        text = text["(empty)".len()..].trim();
                    }
                    let mut clean_text = text.to_owned();
                    if clean_text.starts_with("Thinking...") {
                        clean_text = clean_text["Thinking...".len()..].to_owned();
                    } else if clean_text.starts_with("Thinking:") {
                        if let Some(first_newline_idx) = clean_text.find('\n') {
                            clean_text = clean_text[first_newline_idx + 1..].to_owned();
                        } else {
                            clean_text = clean_text["Thinking:".len()..].to_owned();
                        }
                    } else if clean_text.starts_with("░ Thinking...") {
                        clean_text = clean_text["░ Thinking...".len()..].to_owned();
                    } else if clean_text.starts_with("░ Thinking:") {
                        if let Some(first_newline_idx) = clean_text.find('\n') {
                            clean_text = clean_text[first_newline_idx + 1..].to_owned();
                        } else {
                            clean_text = clean_text["░ Thinking:".len()..].to_owned();
                        }
                    }
                    let final_text = clean_text.trim().to_owned();
                    if !final_text.is_empty() {
                        last_response = Some(final_text);
                        break;
                    }
                }
            }
            if let Some(text) = last_response {
                if copy_to_clipboard(&text).is_ok() {
                    app.status = "Copied last response to clipboard".to_owned();
                }
            } else {
                app.status = "No assistant response to copy".to_owned();
            }
        }
        (KeyCode::Tab, _) => {
            let suggestions = app.command_suggestions();
            if !suggestions.is_empty() {
                app.accept_command_suggestion();
            } else {
                app.chat.shell_focused = !app.chat.shell_focused;
                if app.chat.shell_focused {
                    app.chat.scroll = 0; // Automatically scroll to the bottom to show the last shell block
                    app.status = "Shell/Messages focused. Press Tab to return, or Ctrl+C to abort running command.".to_owned();
                } else {
                    app.status = "Ready".to_owned();
                }
            }
        }
        (KeyCode::Enter, modifiers) if !modifiers.is_empty() => {
            app.chat.insert_char('\n');
        }
        (KeyCode::Backspace, _) => app.chat.remove_char(),
        (KeyCode::Delete, _) => app.chat.delete_char(),
        (KeyCode::Left, _) => app.chat.move_cursor_left(),
        (KeyCode::Right, _) => app.chat.move_cursor_right(),
        (KeyCode::Home, _) => app.chat.move_cursor_start(),
        (KeyCode::End, _) => app.chat.move_cursor_end(),
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
