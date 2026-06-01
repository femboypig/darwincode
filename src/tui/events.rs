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
                SubmitAction::ExecuteFunction { name, args } => {
                    handle_function_action(crate::app::FunctionAction::Execute { name, args }, sender);
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
            SetupField::Theme => {
                app.setup.theme = app.setup.theme.next();
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
                SetupField::Theme => {
                    app.setup.theme = app.setup.theme.next();
                }
                _ => app.setup.push_char(' '),
            }
        }
        (KeyCode::Backspace, _) => app.setup.pop_char(),
        (KeyCode::Delete, _) | (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
            match app.setup.active_field {
                SetupField::ApiKey => app.setup.api_key.clear(),
                SetupField::Model => app.setup.model.clear(),
                SetupField::BaseUrl => app.setup.base_url.clear(),
                _ => {}
            }
        }
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
        (KeyCode::Up, _) if app.setup.active_field == SetupField::Theme => {
            app.setup.theme = app.setup.theme.next();
        }
        (KeyCode::Down, _) if app.setup.active_field == SetupField::Theme => {
            app.setup.theme = app.setup.theme.next();
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

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Quit, key) {
        app.should_quit = true;
        return Ok(());
    }

    if app.keybindings.matches(crate::tui::keybindings::TuiAction::Cancel, key) {
        if matches!(app.pending, Some(crate::app::PendingTask::Generating)) {
            app.cancel_generation();
        }
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
                SubmitAction::ExecuteFunction { name, args } => {
                    handle_function_action(crate::app::FunctionAction::Execute { name, args }, sender);
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
                    if clean_text.starts_with("░ Thinking...") {
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
        (KeyCode::Tab, _) => app.accept_command_suggestion(),
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
