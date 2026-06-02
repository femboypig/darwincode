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
        Screen::Permissions => handle_permissions_key(app, sender, key),
        Screen::Sessions => handle_sessions_key(app, key),
        Screen::AskUser => handle_ask_user_key(app, key),
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

fn handle_permissions_key(app: &mut App, sender: &Sender<WorkerEvent>, key: KeyEvent) -> Result<()> {
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Quit, key) {
        app.cancel_permissions();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key) {
        app.cancel_permissions();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollUp, key) {
        app.permissions.select_previous();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollDown, key) {
        app.permissions.select_next();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Submit, key) {
        if let Some(action) = app.apply_permission_level() {
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
    Ok(())
}

fn handle_setup_key(app: &mut App, _sender: &Sender<WorkerEvent>, key: KeyEvent) -> Result<()> {
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Quit, key) {
        app.should_quit = true;
        return Ok(());
    }

    if app.setup.is_editing {
        if app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key) {
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
                if app.setup.active_field == SetupField::ApiKey && app.setup.api_key.starts_with("sk-") && !old_key.starts_with("sk-") {
                    app.status = "OpenAI key detected. Press Ctrl+A to apply OmniRoute defaults.".to_owned();
                }
            }
            _ => {}
        }
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key) {
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
                    crate::config::PermissionLevel::Guardian => crate::config::PermissionLevel::Safe,
                    crate::config::PermissionLevel::Chaos => crate::config::PermissionLevel::Guardian,
                };
            }
            SetupField::Theme => {
                app.setup.theme = app.setup.theme.next();
            }
            _ => {}
        }
        (KeyCode::Right, _) => match app.setup.active_field {
            SetupField::Model => {
                app.setup.select_next_model();
            }
            SetupField::PermissionLevel => {
                app.setup.permission_level = match app.setup.permission_level {
                    crate::config::PermissionLevel::Safe => crate::config::PermissionLevel::Guardian,
                    crate::config::PermissionLevel::Guardian => crate::config::PermissionLevel::Chaos,
                    crate::config::PermissionLevel::Chaos => crate::config::PermissionLevel::Safe,
                };
            }
            SetupField::Theme => {
                app.setup.theme = app.setup.theme.next();
            }
            _ => {}
        }
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
                    crate::config::PermissionLevel::Safe => crate::config::PermissionLevel::Guardian,
                    crate::config::PermissionLevel::Guardian => crate::config::PermissionLevel::Chaos,
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
        }
        (KeyCode::Char(value), modifiers)
            if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
        {
            match app.setup.active_field {
                SetupField::ApiKey | SetupField::Model | SetupField::BaseUrl => {
                    app.setup.is_editing = true;
                    let old_key = app.setup.api_key.clone();
                    app.setup.push_char(value);
                    if app.setup.active_field == SetupField::ApiKey && app.setup.api_key.starts_with("sk-") && !old_key.starts_with("sk-") {
                        app.status = "OpenAI key detected. Press Ctrl+A to apply OmniRoute defaults.".to_owned();
                    }
                }
                _ => {}
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

fn handle_models_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Quit, key)
        || app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        app.cancel_models();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollUp, key) {
        app.select_previous_model();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollDown, key) {
        app.select_next_model();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Submit, key) {
        app.apply_selected_model();
        return Ok(());
    }
    Ok(())
}

fn handle_sessions_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Quit, key)
        || app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key)
    {
        app.cancel_sessions();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollUp, key) {
        app.sessions.select_previous();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::ScrollDown, key) {
        app.sessions.select_next();
        return Ok(());
    }
    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Submit, key) {
        app.apply_selected_session();
        return Ok(());
    }

    match (key.code, key.modifiers) {
        (KeyCode::Backspace, _) => {
            let mut q = app.sessions.query.clone();
            q.pop();
            app.sessions.update_query(q);
        }
        (KeyCode::Char(c), modifiers) if !modifiers.contains(KeyModifiers::CONTROL) && !modifiers.contains(KeyModifiers::ALT) => {
            let mut q = app.sessions.query.clone();
            q.push(c);
            app.sessions.update_query(q);
        }
        _ => {}
    }
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::process::{Command, Stdio};
    use std::io::Write;

    if cfg!(target_os = "macos") {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("pbcopy failed");
        }
        Ok(())
    } else if cfg!(target_os = "windows") {
        let mut child = Command::new("clip")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("clip failed");
        }
        Ok(())
    } else {
        // Linux / Unix
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let mut tried_wl = false;

        if is_wayland {
            if let Ok(true) = try_copy_wl(text) { return Ok(()) }
            tried_wl = true;
        }

        if let Ok(true) = try_copy_x11(text) { return Ok(()) }

        if !tried_wl
            && let Ok(true) = try_copy_wl(text) { return Ok(()) }

        anyhow::bail!("No working clipboard tool found (tried wl-copy, xclip, xsel)")
    }
}

fn try_copy_wl(text: &str) -> Result<bool> {
    use std::process::{Command, Stdio};
    use std::io::Write;

    let mut child = match Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }

    let status = child.wait()?;
    Ok(status.success())
}

fn try_copy_x11(text: &str) -> Result<bool> {
    use std::process::{Command, Stdio};
    use std::io::Write;

    // Try xclip
    let child_res = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match child_res {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            let status = child.wait()?;
            if status.success() {
                return Ok(true);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    // Try xsel
    let child_res = Command::new("xsel")
        .args(["--clipboard", "--input"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match child_res {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            let status = child.wait()?;
            if status.success() {
                return Ok(true);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    Ok(false)
}

fn read_from_clipboard() -> Result<String> {
    use std::process::Command;

    if cfg!(target_os = "macos") {
        let output = Command::new("pbpaste")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;
        if !output.status.success() {
            anyhow::bail!("pbpaste failed");
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else if cfg!(target_os = "windows") {
        let output = Command::new("powershell.exe")
            .args(["-Command", "Get-Clipboard"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;
        if !output.status.success() {
            anyhow::bail!("powershell Get-Clipboard failed");
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        // Linux / Unix
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let mut tried_wl = false;

        if is_wayland {
            if let Ok(Some(text)) = try_paste_wl() { return Ok(text) }
            tried_wl = true;
        }

        if let Ok(Some(text)) = try_paste_x11() { return Ok(text) }

        if !tried_wl
            && let Ok(Some(text)) = try_paste_wl() { return Ok(text) }

        anyhow::bail!("No working clipboard tool found for pasting")
    }
}

fn try_paste_wl() -> Result<Option<String>> {
    use std::process::{Command, Stdio};

    let output_res = Command::new("wl-paste")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output_res {
        Ok(output) => {
            if output.status.success() {
                Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
            } else {
                Ok(None)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn try_paste_x11() -> Result<Option<String>> {
    use std::process::{Command, Stdio};

    // Try xclip
    let output_res = Command::new("xclip")
        .args(["-o", "-selection", "clipboard"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output_res {
        Ok(output) => {
            if output.status.success() {
                return Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    // Try xsel
    let output_res = Command::new("xsel")
        .args(["--clipboard", "--output"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output_res {
        Ok(output) => {
            if output.status.success() {
                return Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    Ok(None)
}

fn handle_ask_user_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if let Ok(mut guard) = crate::tui::ASK_USER_CHANNEL.lock() {
            if let Some((tx, _, _)) = guard.take() {
                let _ = tx.send("Aborted by user".to_owned());
            }
        }
        app.cancel_generation();
        app.screen = Screen::Chat;
        app.status = "Aborted by user".to_owned();
        return Ok(());
    }

    if app.ask_user.is_custom {
        match key.code {
            KeyCode::Enter => {
                let answer = app.ask_user.custom_input.trim().to_owned();
                if !answer.is_empty() {
                    if let Ok(mut guard) = crate::tui::ASK_USER_CHANNEL.lock() {
                        if let Some((tx, _, _)) = guard.take() {
                            let _ = tx.send(answer);
                        }
                    }
                    app.screen = Screen::Chat;
                    app.status = "Ready".to_owned();
                }
            }
            KeyCode::Esc => {
                if !app.ask_user.options.is_empty() {
                    app.ask_user.is_custom = false;
                }
            }
            KeyCode::Backspace => {
                app.ask_user.custom_input.pop();
            }
            KeyCode::Char(c) => {
                app.ask_user.custom_input.push(c);
            }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Up => {
                app.ask_user.selected_idx = app.ask_user.selected_idx.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = app.ask_user.options.len();
                if app.ask_user.selected_idx < max {
                    app.ask_user.selected_idx += 1;
                }
            }
            KeyCode::Enter => {
                if app.ask_user.selected_idx == app.ask_user.options.len() {
                    app.ask_user.is_custom = true;
                } else if let Some(opt) = app.ask_user.options.get(app.ask_user.selected_idx) {
                    let answer = opt.clone();
                    if let Ok(mut guard) = crate::tui::ASK_USER_CHANNEL.lock() {
                        if let Some((tx, _, _)) = guard.take() {
                            let _ = tx.send(answer);
                        }
                    }
                    app.screen = Screen::Chat;
                    app.status = "Ready".to_owned();
                }
            }
            _ => {}
        }
    }
    Ok(())
}
