use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::mpsc::Sender;

use crate::app::{App, SubmitAction};
use crate::tui::events::common::{
    copy_to_clipboard, pasted_images_dir, read_from_clipboard, read_image_from_clipboard,
    uuid_or_timestamp,
};
use crate::tui::{
    WorkerEvent, handle_function_action, spawn_generation_worker, spawn_models_worker,
};

pub(crate) fn handle_chat_key(
    app: &mut App,
    sender: &Sender<WorkerEvent>,
    key: KeyEvent,
) -> Result<()> {
    if matches!(
        app.pending,
        Some(crate::app::PendingTask::ConfirmFunction { .. })
    ) {
        if app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::Quit, key)
        {
            app.should_quit = true;
            return Ok(());
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(action) = app.answer_function_confirmation(true) {
                    handle_function_action(action, sender);
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(action) = app.answer_function_confirmation(false) {
                    handle_function_action(action, sender);
                }
            }
            _ => {
                if app
                    .keybindings
                    .matches(crate::tui::keybindings::TuiAction::Cancel, key)
                {
                    if let Some(action) = app.answer_function_confirmation(false) {
                        handle_function_action(action, sender);
                    }
                } else if app
                    .keybindings
                    .matches(crate::tui::keybindings::TuiAction::ScrollUp, key)
                {
                    app.confirm_scroll = app.confirm_scroll.saturating_sub(1);
                } else if app
                    .keybindings
                    .matches(crate::tui::keybindings::TuiAction::ScrollDown, key)
                {
                    app.confirm_scroll = app.confirm_scroll.saturating_add(1);
                } else if app
                    .keybindings
                    .matches(crate::tui::keybindings::TuiAction::PageUp, key)
                {
                    app.confirm_scroll = app.confirm_scroll.saturating_sub(10);
                } else if app
                    .keybindings
                    .matches(crate::tui::keybindings::TuiAction::PageDown, key)
                {
                    app.confirm_scroll = app.confirm_scroll.saturating_add(10);
                }
            }
        }
        return Ok(());
    }

    if app.chat.shell_focused {
        let is_ctrl_c = (key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL))
            || app
                .keybindings
                .matches(crate::tui::keybindings::TuiAction::Quit, key);
        let is_esc = app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::Cancel, key);

        if is_ctrl_c || is_esc {
            let mut pid = None;
            if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
                pid = guard.take();
            }
            if let Some(pid) = pid {
                #[cfg(unix)]
                {
                    let _ = std::process::Command::new("kill")
                        .arg("-9")
                        .arg(format!("-{}", pid))
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                #[cfg(not(unix))]
                {
                    let _ = std::process::Command::new("taskkill")
                        .arg("/F")
                        .arg("/PID")
                        .arg(pid.to_string())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
            }
            app.cancel_generation();
            app.chat.shell_focused = false;
            app.chat.focused_shell_session_id = None;
            app.chat.focused_shell_pid = None;
            for m in &mut app.chat.messages {
                if m.is_shell {
                    *m.cached_wrapped.borrow_mut() = None;
                }
            }
            app.status = "Aborted by user".to_owned();
            return Ok(());
        }

        if key.code == KeyCode::Tab {
            app.chat.shell_focused = false;
            for m in &mut app.chat.messages {
                if m.is_shell {
                    *m.cached_wrapped.borrow_mut() = None;
                }
            }
            app.status = "Ready".to_owned();
            return Ok(());
        }

        if app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::ScrollUp, key)
        {
            app.chat.scroll = app.chat.scroll.saturating_add(1);
            return Ok(());
        }
        if app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::ScrollDown, key)
        {
            app.chat.scroll = app.chat.scroll.saturating_sub(1);
            return Ok(());
        }
        if app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::PageUp, key)
        {
            app.chat.scroll = app.chat.scroll.saturating_add(15);
            return Ok(());
        }
        if app
            .keybindings
            .matches(crate::tui::keybindings::TuiAction::PageDown, key)
        {
            app.chat.scroll = app.chat.scroll.saturating_sub(15);
            return Ok(());
        }

        let mut written = false;
        let mut written_pid = None;

        // 1. Try to write to active persistent session stdin
        let active_session_id = if app.chat.shell_focused {
            app.chat.focused_shell_session_id.clone()
        } else {
            crate::tui::ACTIVE_PERSISTENT_SESSION_ID
                .lock()
                .ok()
                .and_then(|g| g.clone())
        };

        if let Some(ref session_id) = active_session_id
            && let Some(registry_mutex) = crate::tui::PERSISTENT_SESSIONS.get()
            && let Ok(mut registry_guard) = registry_mutex.lock()
            && let Some(session) = registry_guard.get_mut(session_id)
        {
            use std::io::Write;
            let data = match key.code {
                KeyCode::Char(c) => Some(c.to_string()),
                KeyCode::Enter => Some("\n".to_owned()),
                KeyCode::Backspace => Some("\x08".to_owned()),
                _ => None,
            };
            if let Some(s) = data {
                let _ = session.stdin.write_all(s.as_bytes());
                let _ = session.stdin.flush();
                written = true;
                written_pid = Some(session.pid);
            }
        }

        // 2. Try to write to non-persistent foreground process stdin
        if !written {
            let mut guard = crate::tui::RUNNING_PROCESS_STDIN.lock();
            if let Some(ref mut stdin) = guard.as_mut().ok().and_then(|g| g.as_mut()) {
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
                    written = true;
                    if let Ok(pid_guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
                        written_pid = *pid_guard;
                    }
                }
            }
        }

        // 3. Try to write to non-persistent background process stdin
        if !written
            && app.chat.shell_focused
            && let Some(pid) = app.chat.focused_shell_pid
        {
            let bg_registry = crate::tui::BACKGROUND_PROCESSES.get();
            if let Some(registry_mutex) = bg_registry
                && let Ok(mut registry_guard) = registry_mutex.lock()
                && let Some(proc) = registry_guard.get_mut(&pid)
                && let Some(ref mut stdin) = proc.stdin
            {
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
                    written = true;
                    written_pid = Some(pid);
                }
            }
        }

        if written {
            // Find the shell message line and truncate "\nRunning...\n" if it's there
            let mut found_msg = None;
            if let Some(ref session_id) = active_session_id {
                found_msg = app
                    .chat
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.is_shell && m.shell_session_id.as_ref() == Some(session_id));
            }
            if found_msg.is_none()
                && let Some(wp) = written_pid
            {
                found_msg = app
                    .chat
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.is_shell && m.shell_pid == Some(wp));
            }
            if found_msg.is_none() {
                found_msg = app.chat.messages.iter_mut().rev().find(|m| m.is_shell);
            }
            if let Some(msg) = found_msg {
                if msg.text.ends_with("\nRunning...\n") {
                    msg.text.truncate(msg.text.len() - 11);
                }
                match key.code {
                    KeyCode::Char(c) => {
                        msg.text.push(c);
                    }
                    KeyCode::Enter => {
                        msg.text.push('\n');
                    }
                    KeyCode::Backspace => {
                        msg.text.pop();
                    }
                    _ => {}
                }
                *msg.cached_wrapped.borrow_mut() = None;
            }
        }

        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Quit, key)
    {
        if app.pending.is_some() || app.chat.focused_shell_pid.is_some() {
            let mut pid = None;
            if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
                pid = guard.take();
            }
            if pid.is_none() {
                pid = app.chat.focused_shell_pid;
            }
            if let Some(pid) = pid {
                #[cfg(unix)]
                {
                    let _ = std::process::Command::new("kill")
                        .arg("-9")
                        .arg(format!("-{}", pid))
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
                #[cfg(not(unix))]
                {
                    let _ = std::process::Command::new("taskkill")
                        .arg("/F")
                        .arg("/PID")
                        .arg(pid.to_string())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
            }
            app.cancel_generation();
            app.chat.shell_focused = false;
            app.chat.focused_shell_session_id = None;
            app.chat.focused_shell_pid = None;
            app.status = "Aborted by user".to_owned();
            return Ok(());
        } else {
            app.should_quit = true;
            return Ok(());
        }
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        let mut pid = None;
        if let Ok(mut guard) = crate::tui::RUNNING_PROCESS_PID.lock() {
            pid = guard.take();
        }
        if pid.is_none() {
            pid = app.chat.focused_shell_pid;
        }
        if let Some(pid) = pid {
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(format!("-{}", pid))
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            #[cfg(not(unix))]
            {
                let _ = std::process::Command::new("taskkill")
                    .arg("/F")
                    .arg("/PID")
                    .arg(pid.to_string())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
        app.cancel_generation();
        app.chat.shell_focused = false;
        app.chat.focused_shell_session_id = None;
        app.chat.focused_shell_pid = None;
        app.status = "Stopped".to_owned();
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ToggleSetup, key)
    {
        app.open_setup();
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ToggleModels, key)
    {
        if let Some(config) = app.begin_load_chat_models() {
            spawn_models_worker(config, sender.clone());
        }
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ToggleSessions, key)
    {
        app.open_sessions();
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Submit, key)
    {
        if let Some(action) = app.submit_chat_input() {
            match action {
                SubmitAction::Generate(request) => {
                    spawn_generation_worker(
                        request.config,
                        request.history,
                        request.cancel_token,
                        request.generation_id,
                        sender.clone(),
                    );
                }
                SubmitAction::LoadModels(config) => {
                    spawn_models_worker(config, sender.clone());
                }
                SubmitAction::ExecuteFunction { name, args, config } => {
                    handle_function_action(
                        crate::app::FunctionAction::Execute { name, args, config },
                        sender,
                    );
                }
            }
        }
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollUp, key)
    {
        if app.chat.input.contains('\n') {
            let old_cursor = app.chat.cursor;
            app.chat.move_cursor_up();
            if app.chat.cursor == old_cursor {
                app.chat.scroll = app.chat.scroll.saturating_add(1);
            }
        } else {
            app.chat.scroll = app.chat.scroll.saturating_add(1);
        }
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::ScrollDown, key)
    {
        if app.chat.input.contains('\n') {
            let old_cursor = app.chat.cursor;
            app.chat.move_cursor_down();
            if app.chat.cursor == old_cursor {
                app.chat.scroll = app.chat.scroll.saturating_sub(1);
            }
        } else {
            app.chat.scroll = app.chat.scroll.saturating_sub(1);
        }
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::PageUp, key)
    {
        app.chat.scroll = app.chat.scroll.saturating_add(15);
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::PageDown, key)
    {
        app.chat.scroll = app.chat.scroll.saturating_sub(15);
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::HistoryUp, key)
    {
        app.chat.navigate_history_up();
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::HistoryDown, key)
    {
        app.chat.navigate_history_down();
        return Ok(());
    }

    if app
        .keybindings
        .matches(crate::tui::keybindings::TuiAction::Paste, key)
    {
        if let Ok(Some(image_bytes)) = read_image_from_clipboard()
            && let Ok(dir) = pasted_images_dir()
        {
            let filename = format!("image_{}.png", uuid_or_timestamp());
            let path = dir.join(&filename);
            if std::fs::write(&path, &image_bytes).is_ok() {
                let ref_str = format!(" @{} ", filename);
                app.chat.insert_text(&ref_str);
                app.status = format!("Pasted image saved to {}", filename);
                return Ok(());
            }
        }
        if let Ok(text) = read_from_clipboard() {
            app.chat.insert_text(&text);
        }
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
                for m in &mut app.chat.messages {
                    if m.is_shell {
                        *m.cached_wrapped.borrow_mut() = None;
                    }
                }
                if app.chat.shell_focused {
                    let last_shell = app.chat.messages.iter().rev().find(|m| m.is_shell);
                    if let Some(msg) = last_shell {
                        app.chat.focused_shell_session_id = msg.shell_session_id.clone();
                        app.chat.focused_shell_pid = msg.shell_pid;
                    } else {
                        app.chat.focused_shell_session_id = None;
                        app.chat.focused_shell_pid = None;
                    }
                    app.chat.scroll = 0; // Automatically scroll to the bottom to show the last shell block
                    app.status = "Shell/Messages focused. Press Tab to return, or Ctrl+C to abort running command.".to_owned();
                } else {
                    app.chat.focused_shell_session_id = None;
                    app.chat.focused_shell_pid = None;
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
