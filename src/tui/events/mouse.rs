use std::sync::mpsc::Sender;
use anyhow::Result;
use crossterm::event::{MouseEvent, MouseButton, MouseEventKind};
use crate::app::App;
use crate::tui::WorkerEvent;
use crate::tui::{
    ACTIVE_PERSISTENT_SESSION_ID, RUNNING_PROCESS_PID, BACKGROUND_PROCESSES, spawn_models_worker,
};

pub(crate) fn handle_mouse_event(
    app: &mut App,
    sender: &Sender<WorkerEvent>,
    mouse_event: MouseEvent,
) -> Result<()> {
    match mouse_event.kind {
        MouseEventKind::ScrollUp => {
            if matches!(
                app.proc.pending,
                Some(crate::app::PendingTask::ConfirmFunction { .. })
            ) {
                app.ui.confirm_scroll
                    .set(app.ui.confirm_scroll.get().saturating_sub(1));
            } else {
                app.chat.scroll = app.chat.scroll.saturating_add(1);
            }
            update_selection_on_scroll(app, mouse_event.column, mouse_event.row);
        }
        MouseEventKind::ScrollDown => {
            if matches!(
                app.proc.pending,
                Some(crate::app::PendingTask::ConfirmFunction { .. })
            ) {
                app.ui.confirm_scroll
                    .set(app.ui.confirm_scroll.get().saturating_add(1));
            } else {
                app.chat.scroll = app.chat.scroll.saturating_sub(1);
            }
            update_selection_on_scroll(app, mouse_event.column, mouse_event.row);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let click_x = mouse_event.column;
            let click_y = mouse_event.row;

            if app.ui.screen == crate::app::Screen::Setup {
                if let Some(rect) = app.ui.setup.modal_area.get()
                    && click_x >= rect.x
                    && click_x < rect.x + rect.width
                    && click_y >= rect.y
                    && click_y < rect.y + rect.height
                {
                    let margin = 1u16;
                    let content_y = rect.y + margin;
                    let content_height = rect.height.saturating_sub(margin * 2);

                    let body_y = content_y + 2;
                    let body_height = content_height.saturating_sub(5) as usize;

                    if click_y >= body_y && click_y < body_y + body_height as u16 {
                        let offset = (click_y - body_y) as usize;
                        let total_lines = 10;
                        let viewport = body_height;
                        let active_idx = app.ui.setup.active_field.index();
                        let start = if total_lines <= viewport || active_idx < viewport / 2 {
                            0
                        } else if active_idx >= total_lines - viewport / 2 {
                            total_lines - viewport
                        } else {
                            active_idx - viewport / 2
                        };

                        let idx = start + offset;
                        if idx < total_lines {
                            let field = crate::app::SetupField::from_index(idx);
                            app.ui.setup.active_field = field;

                            match field {
                                crate::app::SetupField::Save => {
                                    if let Err(error) = app.save_setup() {
                                        app.status = error.to_string();
                                    }
                                }
                                crate::app::SetupField::EnableCodebase => {
                                    app.ui.setup.enable_codebase_tools =
                                        !app.ui.setup.enable_codebase_tools;
                                }
                                crate::app::SetupField::EnableBash => {
                                    app.ui.setup.enable_bash_tools =
                                        !app.ui.setup.enable_bash_tools;
                                }
                                crate::app::SetupField::RespectIgnoreRules => {
                                    app.ui.setup.respect_ignore_rules =
                                        !app.ui.setup.respect_ignore_rules;
                                }
                                crate::app::SetupField::ShowThoughts => {
                                    app.ui.setup.show_thoughts = !app.ui.setup.show_thoughts;
                                }
                                crate::app::SetupField::Theme => {
                                    app.ui.setup.theme = app.ui.setup.theme.next();
                                }
                                crate::app::SetupField::PermissionLevel => {
                                    app.ui.setup.permission_level = match app
                                        .ui
                                        .setup
                                        .permission_level
                                    {
                                        crate::config::PermissionLevel::Safe => {
                                            crate::config::PermissionLevel::Guardian
                                        }
                                        crate::config::PermissionLevel::Guardian => {
                                            crate::config::PermissionLevel::Chaos
                                        }
                                        crate::config::PermissionLevel::Chaos => {
                                            crate::config::PermissionLevel::Safe
                                        }
                                    };
                                }
                                crate::app::SetupField::ApiKey
                                | crate::app::SetupField::Model
                                | crate::app::SetupField::BaseUrl => {
                                    app.ui.setup.is_editing = true;
                                }
                            }
                        }
                    }
                }
                return Ok(());
            }

            if let Some(rect) = app.chat.mode_area.get()
                && click_x >= rect.x
                && click_x < rect.x + rect.width
                && click_y == rect.y
            {
                app.chat.selection = None;
                app.chat.last_mouse_drag_pos = None;
                app.toggle_dev_mode();
                return Ok(());
            }

            if let Some(rect) = app.chat.model_area.get()
                && click_x >= rect.x
                && click_x < rect.x + rect.width
                && click_y == rect.y
            {
                app.chat.selection = None;
                app.chat.last_mouse_drag_pos = None;
                if app.ui.model_picker_open {
                    app.cancel_models();
                } else if let Some(config) = app.begin_load_chat_models() {
                    spawn_models_worker(config, sender.clone());
                }
                return Ok(());
            }

            if let Some(rect) = app.chat.messages_area.get()
                && click_x >= rect.x
                && click_x < rect.x + rect.width
                && click_y >= rect.y
                && click_y < rect.y + rect.height
            {
                let total_lines = app
                    .chat
                    .message_line_ranges
                    .borrow()
                    .last()
                    .map(|(_, _, end)| *end)
                    .unwrap_or(0);
                let viewport_height = rect.height as usize;
                let max_scroll = total_lines.saturating_sub(viewport_height);
                let scroll_offset = (app.chat.scroll as usize).min(max_scroll);
                let scroll_y = max_scroll.saturating_sub(scroll_offset);

                let clicked_line = scroll_y + usize::from(click_y - rect.y);

                let mut clicked_msg_idx = None;
                for &(msg_idx, start_line, end_line) in
                    app.chat.message_line_ranges.borrow().iter()
                {
                    if clicked_line >= start_line && clicked_line < end_line {
                        clicked_msg_idx = Some(msg_idx);
                        break;
                    }
                }

                if let Some(msg_idx) = clicked_msg_idx
                    && app.chat.messages[msg_idx].is_shell
                {
                    app.chat.selection = None;
                    app.chat.last_mouse_drag_pos = None;
                    if let Some(session_id) =
                        app.chat.messages[msg_idx].shell_session_id.clone()
                    {
                        {
                            let mut guard = ACTIVE_PERSISTENT_SESSION_ID.lock();
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
                        if let Some(&(_, start_line, end_line)) = app
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
                            let viewport_height = rect.height as usize;
                            let max_scroll =
                                total_lines.saturating_sub(viewport_height);
                            let msg_height = end_line.saturating_sub(start_line);
                            let mid_line = start_line + msg_height / 2;
                            let target_scroll_y =
                                mid_line.saturating_sub(viewport_height / 2);
                            let scroll_val = max_scroll.saturating_sub(target_scroll_y);
                            app.chat.scroll = u16::try_from(scroll_val).unwrap_or(u16::MAX);
                            scrolled = true;
                        }
                        if !scrolled {
                            app.chat.scroll = 0;
                        }

                        *app.chat.message_line_ranges.borrow_mut() = Vec::new();
                        app.status = "Ready".to_owned();
                    } else {
                        let clicked_pid = app.chat.messages[msg_idx].shell_pid;
                        let fg_pid = *RUNNING_PROCESS_PID.lock();
                        let is_bg_alive = if let Some(pid) = clicked_pid {
                            let bg_registry = BACKGROUND_PROCESSES.get();
                            let registry_guard = bg_registry.as_ref().map(|m| m.lock());
                            if let Some(guard) = registry_guard
                                && let Some(proc) = guard.get(&pid)
                            {
                                proc.exit_status.lock().is_none()
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        if let Some(pid) = fg_pid
                            && Some(pid) == clicked_pid
                        {
                            {
                                let mut guard = ACTIVE_PERSISTENT_SESSION_ID.lock();
                                *guard = None;
                            }
                            app.chat.focused_shell_session_id = None;
                            app.chat.focused_shell_pid = Some(pid);
                            app.chat.shell_focused = true;

                            for m in &mut app.chat.messages {
                                if m.is_shell {
                                    *m.cached_wrapped.borrow_mut() = None;
                                }
                            }

                            let mut scrolled = false;
                            if let Some(&(_, start_line, end_line)) = app
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
                                let viewport_height = rect.height as usize;
                                let max_scroll =
                                    total_lines.saturating_sub(viewport_height);
                                let msg_height = end_line.saturating_sub(start_line);
                                let mid_line = start_line + msg_height / 2;
                                let target_scroll_y =
                                    mid_line.saturating_sub(viewport_height / 2);
                                let scroll_val =
                                    max_scroll.saturating_sub(target_scroll_y);
                                app.chat.scroll = u16::try_from(scroll_val).unwrap_or(u16::MAX);
                                scrolled = true;
                            }
                            if !scrolled {
                                app.chat.scroll = 0;
                            }

                            *app.chat.message_line_ranges.borrow_mut() = Vec::new();
                            app.status = "Ready".to_owned();
                        } else if is_bg_alive && let Some(pid) = clicked_pid {
                            {
                                let mut guard = ACTIVE_PERSISTENT_SESSION_ID.lock();
                                *guard = None;
                            }
                            app.chat.focused_shell_session_id = None;
                            app.chat.focused_shell_pid = Some(pid);
                            app.chat.shell_focused = true;

                            for m in &mut app.chat.messages {
                                if m.is_shell {
                                    *m.cached_wrapped.borrow_mut() = None;
                                }
                            }

                            let mut scrolled = false;
                            if let Some(&(_, start_line, end_line)) = app
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
                                let viewport_height = rect.height as usize;
                                let max_scroll =
                                    total_lines.saturating_sub(viewport_height);
                                let msg_height = end_line.saturating_sub(start_line);
                                let mid_line = start_line + msg_height / 2;
                                let target_scroll_y =
                                    mid_line.saturating_sub(viewport_height / 2);
                                let scroll_val =
                                    max_scroll.saturating_sub(target_scroll_y);
                                app.chat.scroll = u16::try_from(scroll_val).unwrap_or(u16::MAX);
                                scrolled = true;
                            }
                            if !scrolled {
                                app.chat.scroll = 0;
                            }

                            *app.chat.message_line_ranges.borrow_mut() = Vec::new();
                            app.status = "Ready".to_owned();
                        } else {
                            app.chat
                                .messages
                                .push(crate::app::MessageLine::error(
                                    "This shell process has already terminated or does not support input.".to_owned()
                                ));
                        }
                    }
                } else if let Some(msg_idx) = clicked_msg_idx {
                    let message = &app.chat.messages[msg_idx];
                    if message.author == "Darwin" && !message.is_shell && !message.is_tool {
                        if let Some(&(_, start_line, _)) = app
                            .chat
                            .message_line_ranges
                            .borrow()
                            .iter()
                            .find(|&&(idx, _, _)| idx == msg_idx)
                        {
                            let rel_line = clicked_line.saturating_sub(start_line);
                            let text_start_x = rect.x + 6;
                            let rel_col = if click_x >= text_start_x {
                                usize::from(click_x - text_start_x)
                            } else {
                                0
                            };

                            app.chat.selection = Some(crate::app::chat::MessageSelection {
                                msg_idx,
                                start_line: rel_line,
                                start_col: rel_col,
                                end_line: rel_line,
                                end_col: rel_col,
                            });
                            app.chat.last_mouse_drag_pos = Some((click_x, click_y));
                        }
                    } else {
                        app.chat.selection = None;
                        app.chat.last_mouse_drag_pos = None;
                    }
                } else {
                    app.chat.selection = None;
                    app.chat.last_mouse_drag_pos = None;
                }
            } else {
                app.chat.selection = None;
                app.chat.last_mouse_drag_pos = None;
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(rect) = app.chat.messages_area.get() {
                let click_x = mouse_event.column;
                let click_y = mouse_event.row;

                app.chat.last_mouse_drag_pos = Some((click_x, click_y));

                // Handle scrolling if dragging outside the viewport vertical boundaries
                if click_y < rect.y {
                    app.chat.scroll = app.chat.scroll.saturating_add(1);
                } else if click_y >= rect.y + rect.height {
                    app.chat.scroll = app.chat.scroll.saturating_sub(1);
                }

                if let Some(ref mut sel) = app.chat.selection {
                    let clamped_y = click_y.clamp(rect.y, rect.y + rect.height - 1);

                    let total_lines = app
                        .chat
                        .message_line_ranges
                        .borrow()
                        .last()
                        .map(|(_, _, end)| *end)
                        .unwrap_or(0);
                    let viewport_height = rect.height as usize;
                    let max_scroll = total_lines.saturating_sub(viewport_height);
                    let scroll_offset = (app.chat.scroll as usize).min(max_scroll);
                    let scroll_y = max_scroll.saturating_sub(scroll_offset);

                    let clicked_line = scroll_y + usize::from(clamped_y - rect.y);

                    if let Some(&(_, start_line, end_line)) = app
                        .chat
                        .message_line_ranges
                        .borrow()
                        .iter()
                        .find(|&&(idx, _, _)| idx == sel.msg_idx)
                    {
                        let clamped_line = clicked_line.clamp(start_line, end_line.saturating_sub(1));
                        let rel_line = clamped_line.saturating_sub(start_line);

                        let text_start_x = rect.x + 6;
                        let rel_col = if click_x >= text_start_x {
                            usize::from(click_x - text_start_x)
                        } else {
                            0
                        };

                        sel.end_line = rel_line;
                        sel.end_col = rel_col;
                    }
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.chat.last_mouse_drag_pos = None;
            if let Some(ref sel) = app.chat.selection
                && (sel.start_line != sel.end_line || sel.start_col != sel.end_col)
            {
                let message = &app.chat.messages[sel.msg_idx];
                let text_to_copy = extract_selected_text(message, sel);
                if !text_to_copy.is_empty()
                    && crate::tui::events::common::copy_to_clipboard(&text_to_copy).is_ok()
                {
                    app.status = "Copied selection to clipboard".to_owned();
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn get_line_text_excluding_margin(line: &ratatui::text::Line<'_>) -> String {
    let mut s = String::new();
    for span in &line.spans {
        s.push_str(&span.content);
    }
    if s.starts_with("    ") {
        s.chars().skip(4).collect()
    } else {
        s
    }
}

fn extract_selected_text(
    message: &crate::app::MessageLine,
    selection: &crate::app::chat::MessageSelection,
) -> String {
    let cache = message.cached_wrapped.borrow();
    let lines = if let Some((_, _, ref lines)) = *cache {
        lines
    } else {
        return String::new();
    };

    let (min_line, min_col, max_line, max_col) = selection.normalized();

    let mut result = String::new();
    for line_idx in min_line..=max_line {
        if line_idx >= lines.len() {
            break;
        }

        let line_text = get_line_text_excluding_margin(&lines[line_idx]);
        let line_chars: Vec<char> = line_text.chars().collect();

        if min_line == max_line {
            let start = min_col.min(line_chars.len());
            let end = max_col.min(line_chars.len());
            if start < end {
                result.push_str(&line_chars[start..end].iter().collect::<String>());
            }
        } else if line_idx == min_line {
            let start = min_col.min(line_chars.len());
            if start < line_chars.len() {
                result.push_str(&line_chars[start..].iter().collect::<String>());
            }
            result.push('\n');
        } else if line_idx == max_line {
            let end = max_col.min(line_chars.len());
            if end > 0 {
                result.push_str(&line_chars[..end].iter().collect::<String>());
            }
        } else {
            result.push_str(&line_text);
            result.push('\n');
        }
    }

    result
}

pub(crate) fn update_selection_on_scroll(app: &mut App, click_x: u16, click_y: u16) {
    if let Some(rect) = app.chat.messages_area.get()
        && let Some(ref mut sel) = app.chat.selection
    {
        let clamped_y = click_y.clamp(rect.y, rect.y + rect.height - 1);
            let total_lines = app
                .chat
                .message_line_ranges
                .borrow()
                .last()
                .map(|(_, _, end)| *end)
                .unwrap_or(0);
            let viewport_height = rect.height as usize;
            let max_scroll = total_lines.saturating_sub(viewport_height);
            let scroll_offset = (app.chat.scroll as usize).min(max_scroll);
            let scroll_y = max_scroll.saturating_sub(scroll_offset);

            let clicked_line = scroll_y + usize::from(clamped_y - rect.y);
            if let Some(&(_, start_line, end_line)) = app
                .chat
                .message_line_ranges
                .borrow()
                .iter()
                .find(|&&(idx, _, _)| idx == sel.msg_idx)
            {
                let clamped_line = clicked_line.clamp(start_line, end_line.saturating_sub(1));
                let rel_line = clamped_line.saturating_sub(start_line);

                let text_start_x = rect.x + 6;
                let rel_col = if click_x >= text_start_x {
                    usize::from(click_x - text_start_x)
                } else {
                    0
                };

                sel.end_line = rel_line;
                sel.end_col = rel_col;
            }
        }
    }
