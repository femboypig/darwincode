use crate::app::core::App;
use crate::app::chat::MessageLine;

pub fn run(app: &mut App, session_arg: Option<String>) {
    if let Some(session_id) = session_arg {
        // Switch/focus to a specific session or active process
        let mut found = false;
        let registry = crate::tui::PERSISTENT_SESSIONS
            .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
        let has_session = {
            let map = registry.lock().unwrap();
            map.contains_key(session_id.as_str())
        };

        let is_bg_process = {
            let bg_registry = crate::tui::BACKGROUND_PROCESSES.get_or_init(|| {
                std::sync::Mutex::new(std::collections::HashMap::new())
            });
            let map = bg_registry.lock().unwrap();
            map.keys().any(|k| k.to_string() == session_id)
        };

        if has_session {
            if let Ok(mut guard) = crate::tui::ACTIVE_PERSISTENT_SESSION_ID.lock() {
                let opt: &mut Option<String> = &mut guard;
                *opt = Some(session_id.clone());
            }
            app.chat.focused_shell_session_id = Some(session_id.clone());
            app.chat.focused_shell_pid = None;
            app.chat.shell_focused = true;

            for m in &mut app.chat.messages {
                if m.is_shell {
                    *m.cached_wrapped.borrow_mut() = None;
                }
            }

            let mut scrolled = false;
            let target_msg_idx = app
                .chat
                .messages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, m)| {
                    m.is_shell && m.shell_session_id.as_ref() == Some(&session_id)
                })
                .map(|(idx, _)| idx);
            if let Some(msg_idx) = target_msg_idx
                && let Some(&(_, start_line, end_line)) = app
                    .chat
                    .message_line_ranges
                    .borrow()
                    .iter()
                    .find(|&&(idx, _, _)| idx == msg_idx)
            {
                let total_lines = app
                    .chat
                    .message_line_ranges
                    .borrow()
                    .last()
                    .map(|(_, _, end)| *end)
                    .unwrap_or(0);
                let viewport_height = app
                    .chat
                    .messages_area
                    .get()
                    .map(|r| r.height as usize)
                    .unwrap_or(20);
                let max_scroll = total_lines.saturating_sub(viewport_height);
                let msg_height = end_line.saturating_sub(start_line);
                let mid_line = start_line + msg_height / 2;
                let target_scroll_y = mid_line.saturating_sub(viewport_height / 2);
                let scroll_val = max_scroll.saturating_sub(target_scroll_y);
                app.chat.scroll = scroll_val as u16;
                scrolled = true;
            }
            if !scrolled {
                app.chat.scroll = 0;
            }

            *app.chat.message_line_ranges.borrow_mut() = Vec::new();
            app.status = "Ready".to_owned();
            found = true;
        } else if let Some(pid) = *crate::tui::RUNNING_PROCESS_PID.lock()
            && pid.to_string() == session_id
        {
            *crate::tui::ACTIVE_PERSISTENT_SESSION_ID.lock() = None;
            app.chat.focused_shell_session_id = None;
            app.chat.focused_shell_pid = Some(pid);
            app.chat.shell_focused = true;

            for m in &mut app.chat.messages {
                if m.is_shell {
                    *m.cached_wrapped.borrow_mut() = None;
                }
            }

            let mut scrolled = false;
            let target_msg_idx = app
                .chat
                .messages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, m)| m.is_shell && m.shell_pid == Some(pid))
                .map(|(idx, _)| idx);
            if let Some(msg_idx) = target_msg_idx
                && let Some(&(_, start_line, end_line)) = app
                    .chat
                    .message_line_ranges
                    .borrow()
                    .iter()
                    .find(|&&(idx, _, _)| idx == msg_idx)
            {
                let total_lines = app
                    .chat
                    .message_line_ranges
                    .borrow()
                    .last()
                    .map(|(_, _, end)| *end)
                    .unwrap_or(0);
                let viewport_height = app
                    .chat
                    .messages_area
                    .get()
                    .map(|r| r.height as usize)
                    .unwrap_or(20);
                let max_scroll = total_lines.saturating_sub(viewport_height);
                let msg_height = end_line.saturating_sub(start_line);
                let mid_line = start_line + msg_height / 2;
                let target_scroll_y = mid_line.saturating_sub(viewport_height / 2);
                let scroll_val = max_scroll.saturating_sub(target_scroll_y);
                app.chat.scroll = scroll_val as u16;
                scrolled = true;
            }
            if !scrolled {
                app.chat.scroll = 0;
            }

            *app.chat.message_line_ranges.borrow_mut() = Vec::new();
            app.status = "Ready".to_owned();
            found = true;
        } else if is_bg_process {
            let pid = session_id.parse::<u32>().unwrap();
            *crate::tui::ACTIVE_PERSISTENT_SESSION_ID.lock() = None;
            app.chat.focused_shell_session_id = None;
            app.chat.focused_shell_pid = Some(pid);
            app.chat.shell_focused = true;

            for m in &mut app.chat.messages {
                if m.is_shell {
                    *m.cached_wrapped.borrow_mut() = None;
                }
            }

            let mut scrolled = false;
            let target_msg_idx = app
                .chat
                .messages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, m)| m.is_shell && m.shell_pid == Some(pid))
                .map(|(idx, _)| idx);
            if let Some(msg_idx) = target_msg_idx
                && let Some(&(_, start_line, end_line)) = app
                    .chat
                    .message_line_ranges
                    .borrow()
                    .iter()
                    .find(|&&(idx, _, _)| idx == msg_idx)
            {
                let total_lines = app
                    .chat
                    .message_line_ranges
                    .borrow()
                    .last()
                    .map(|(_, _, end)| *end)
                    .unwrap_or(0);
                let viewport_height = app
                    .chat
                    .messages_area
                    .get()
                    .map(|r| r.height as usize)
                    .unwrap_or(20);
                let max_scroll = total_lines.saturating_sub(viewport_height);
                let msg_height = end_line.saturating_sub(start_line);
                let mid_line = start_line + msg_height / 2;
                let target_scroll_y = mid_line.saturating_sub(viewport_height / 2);
                let scroll_val = max_scroll.saturating_sub(target_scroll_y);
                app.chat.scroll = scroll_val as u16;
                scrolled = true;
            }
            if !scrolled {
                app.chat.scroll = 0;
            }

            *app.chat.message_line_ranges.borrow_mut() = Vec::new();
            app.status = "Ready".to_owned();
            found = true;
        }
        if !found {
            app.chat.messages.push(MessageLine::error(format!(
                "Shell session or active process '{}' not found or cannot be focused.",
                session_id
            )));
        }
    } else {
        // List all active sessions
        let registry = crate::tui::PERSISTENT_SESSIONS
            .get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

        let mut session_infos = Vec::new();

        // 1. Persistent Sessions
        {
            let map = registry.lock();
            for (id, session) in map.iter() {
                let is_running =
                    matches!(session.child.lock().try_wait(), Ok(None));
                if is_running {
                    let active_str = if app.chat.shell_focused
                        && app.chat.focused_shell_session_id.as_ref() == Some(id)
                    {
                        " (focused)"
                    } else {
                        ""
                    };
                    session_infos.push(format!(
                        "- **Persistent Session: {}** (PID: {}) [active]{}",
                        id, session.pid, active_str
                    ));
                }
            }
        }

        // 2. Non-persistent Background Processes
        let bg_registry = crate::tui::BACKGROUND_PROCESSES
            .get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()));
        {
            let map = bg_registry.lock();
            for (pid, proc) in map.iter() {
                let is_running = proc.exit_status.lock().is_none();
                if is_running {
                    session_infos.push(format!(
                        "- **Background Process: {}** (PID: {}) [active]",
                        proc._command, pid
                    ));
                }
            }
        }

        // 3. Foreground Process
        if let Some(pid) = *crate::tui::RUNNING_PROCESS_PID.lock()
        {
            let is_focused =
                app.chat.shell_focused && app.chat.focused_shell_pid == Some(pid);
            let active_str = if is_focused { " (focused)" } else { "" };
            session_infos.push(format!(
                "- **Foreground Process** (PID: {}) [active]{}",
                pid, active_str
            ));
        }

        if session_infos.is_empty() {
            app.chat.messages.push(MessageLine::info(
                "No active shell sessions at this time.".to_owned(),
            ));
        } else {
            session_infos.sort();
            let info_text = format!(
                "Active shell sessions:\n{}\nUse `/shell [session_id_or_pid]` to focus a session.",
                session_infos.join("\n")
            );
            app.chat.messages.push(MessageLine::info(info_text));
        }
    }
}
