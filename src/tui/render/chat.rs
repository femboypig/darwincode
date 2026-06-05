use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, List, ListItem, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};

use crate::app::App;
use crate::app::chat::TodoItem;
use crate::tui::render::icons::icons;
use crate::tui::render::logo::{logo_fits, logo_lines, logo_lines_for_area, welcome_lines};
use crate::tui::render::{get_theme, render_statusbar};
use crate::tui::syntax::{parse_markdown_lines, wrap_lines, wrap_text_to_lines};

pub(crate) fn render_chat(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let suggestions = app.command_suggestions();
    let suggestion_height = if suggestions.is_empty() {
        0
    } else {
        suggestions.len().min(10) as u16
    };

    let has_todos = !app.chat.todos.is_empty() && !app.chat.messages.is_empty();
    let (left_pane, right_pane, statusbar_area) = if has_todos && area.width >= 50 {
        let sidebar_width = if area.width >= 90 { 36 } else { 30 };
        let horizontal_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(sidebar_width)])
            .split(area);
        let left_full_area = horizontal_split[0];
        let right_pane = horizontal_split[1];

        let vertical_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1), // Status bar
                Constraint::Length(1), // Bottom spacer (empty line at bottom)
            ])
            .split(left_full_area);

        (vertical_split[0], Some(right_pane), vertical_split[1])
    } else {
        let vertical_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1), // Status bar
                Constraint::Length(1), // Bottom spacer (empty line at bottom)
            ])
            .split(area);
        (vertical_split[0], None, vertical_split[1])
    };

    // Text wrapped for input: padding of 1 on each side inside the text block,
    // plus margin(2) and blue line(1) on the left side of text block,
    // plus margin(2) on the right side of text block. Total horizontal spacing is 7.
    let input_inner_w = if app.chat.messages.is_empty() {
        // Centered welcome input box is 70% of screen width of left_pane
        let centered_w = (left_pane.width as u32 * 70 / 100) as u16;
        centered_w.saturating_sub(7).max(1)
    } else {
        left_pane.width.saturating_sub(7).max(1)
    };
    let wrapped_lines = wrap_text_to_lines(app.chat.input.as_str(), input_inner_w as usize);
    let display_lines = wrapped_lines.len() as u16;
    let input_height = if app.screen == crate::app::Screen::AskUser {
        let q_wrapped = wrap_text_to_lines(app.ask_user.question.as_str(), input_inner_w as usize);
        let q_lines = q_wrapped.len() as u16;
        let opt_count = app.ask_user.options.len() as u16;
        let custom_lines = if app.ask_user.is_custom {
            if app.ask_user.custom_input.is_empty() {
                1
            } else {
                app.ask_user.custom_input.split('\n').count() as u16
            }
        } else {
            0
        };
        // q_lines + blank + opts (label only) + custom + 1 blank + 1 footer + 1 blank + 2 padding
        (q_lines + 1 + opt_count + custom_lines + 1 + 1 + 1 + 2).max(6)
    } else if app.screen == crate::app::Screen::Permissions {
        let opts = crate::app::PermissionPickerState::options();
        let opt_count = opts.len() as u16;
        // title + blank + 2*opts + 1 blank + 1 footer + 2 padding
        (1 + 1 + opt_count * 2 + 1 + 1 + 2).max(6)
    } else {
        // Input block height: display_lines (min 1, max 5) + 1 top + 1 spacer + 1 mode/model + 1 bottom
        display_lines.clamp(1, 5) + 4
    };

    let (messages_area, suggestions_area, queue_area, input_area, logo_area, tips_area) =
        if app.chat.messages.is_empty() {
            let active_theme = crate::tui::render::get_active_theme(app);
            let logo_lines = logo_lines(Style::default().fg(active_theme.primary));
            let logo_fits_flag = logo_fits(&logo_lines, left_pane.width, 5);

            let remaining_height = left_pane
                .height
                .saturating_sub(input_height + suggestion_height);
            let half_height = remaining_height / 2;
            let top_height = half_height;
            let bottom_height = remaining_height.saturating_sub(half_height);

            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(top_height),
                    Constraint::Length(suggestion_height),
                    Constraint::Length(input_height),
                    Constraint::Length(bottom_height),
                ])
                .split(left_pane);

            let logo_spacer = if top_height >= 8 {
                3
            } else if top_height >= 7 {
                2
            } else if top_height >= 6 {
                1
            } else {
                0
            };

            let top_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(5), // Logo (5 lines)
                    Constraint::Length(logo_spacer),
                ])
                .split(main_chunks[0]);

            let bottom_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Tips line
                    Constraint::Min(0),
                ])
                .split(main_chunks[3]);

            let centered_suggestions_box = if suggestion_height > 0 {
                Some(
                    Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Percentage(15),
                            Constraint::Percentage(70),
                            Constraint::Percentage(15),
                        ])
                        .split(main_chunks[1])[1],
                )
            } else {
                None
            };

            let centered_input_box = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(15),
                    Constraint::Percentage(70),
                    Constraint::Percentage(15),
                ])
                .split(main_chunks[2])[1];

            let logo_area = if logo_fits_flag && top_height >= 5 {
                Some(top_chunks[1])
            } else {
                None
            };

            let tips_area = if bottom_height >= 2 {
                Some(bottom_chunks[1])
            } else {
                None
            };

            (
                None,
                centered_suggestions_box,
                None,
                Some(centered_input_box),
                logo_area,
                tips_area,
            )
        } else {
            let queue_height = if app.chat.message_queue.is_empty() {
                0
            } else {
                (app.chat.message_queue.len().min(3) as u16) + 1
            };

            let spacer_height = if suggestion_height > 0 || queue_height > 0 {
                0
            } else {
                1
            };

            let normal_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(4),
                    Constraint::Length(suggestion_height),
                    Constraint::Length(queue_height),
                    Constraint::Length(spacer_height), // dynamic vertical spacer
                    Constraint::Length(input_height),
                ])
                .split(left_pane);

            (
                Some(normal_chunks[0]),
                if suggestion_height > 0 {
                    Some(normal_chunks[1])
                } else {
                    None
                },
                if queue_height > 0 {
                    Some(normal_chunks[2])
                } else {
                    None
                },
                Some(normal_chunks[4]),
                None,
                None,
            )
        };

    if let Some(logo_area) = logo_area {
        let active_theme = crate::tui::render::get_active_theme(app);
        let logo = logo_lines_for_area(
            Style::default().fg(active_theme.primary),
            logo_area.width,
            5,
        );
        frame.render_widget(Paragraph::new(logo).alignment(Alignment::Center), logo_area);
    }

    if let Some(tips_area) = tips_area {
        let active_theme = crate::tui::render::get_active_theme(app);
        let tips_line = Line::from(vec![
            Span::styled(
                "Ctrl+S",
                Style::default()
                    .fg(Color::Rgb(236, 72, 153))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Setup  •  ", Style::default().fg(active_theme.text_muted)),
            Span::styled(
                "Ctrl+P",
                Style::default()
                    .fg(Color::Rgb(168, 85, 247))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Model  •  ", Style::default().fg(active_theme.text_muted)),
            Span::styled(
                "/help",
                Style::default()
                    .fg(Color::Rgb(59, 130, 246))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Help", Style::default().fg(active_theme.text_muted)),
        ]);
        frame.render_widget(
            Paragraph::new(tips_line).alignment(Alignment::Center),
            tips_area,
        );
    }

    if let Some(messages_area) = messages_area {
        if !app.chat.todos.is_empty() && right_pane.is_none() {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(8)])
                .split(messages_area);
            render_messages(frame, app, layout[0]);
            render_todos(frame, app, layout[1]);
        } else {
            render_messages(frame, app, messages_area);
        }
    }

    if let Some(suggestions_area) = suggestions_area {
        let margin = 2;
        let flat_suggestions_area = Rect {
            x: suggestions_area.x.saturating_add(margin),
            y: suggestions_area.y,
            width: suggestions_area.width.saturating_sub(margin * 2),
            height: suggestions_area.height,
        };
        render_command_suggestions(frame, app, flat_suggestions_area);
    }

    if let Some(queue_area) = queue_area {
        let mut queue_lines = vec![Line::from(Span::styled(
            " Queued Prompts: ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ))];
        for (idx, item) in app.chat.message_queue.iter().enumerate().take(3) {
            let truncated_item = if item.chars().count() > 60 {
                let s: String = item.chars().take(57).collect();
                format!("{}...", s)
            } else {
                item.clone()
            };
            queue_lines.push(Line::from(vec![
                Span::styled(
                    format!("  [{}] ", idx + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(truncated_item, Style::default().fg(Color::DarkGray)),
            ]));
        }
        if app.chat.message_queue.len() > 3 {
            let remaining = app.chat.message_queue.len() - 3;
            queue_lines.push(Line::from(Span::styled(
                format!("  ... and {} more", remaining),
                Style::default().fg(Color::DarkGray),
            )));
        }
        frame.render_widget(Paragraph::new(queue_lines), queue_area);
    }

    let input_box = input_area.unwrap();

    // --- Render input paragraph with scroll ---
    let margin = 2;
    let flat_input_area = Rect {
        x: input_box.x.saturating_add(margin),
        y: input_box.y,
        width: input_box.width.saturating_sub(margin * 2),
        height: input_box.height,
    };

    let active_theme = crate::tui::render::get_active_theme(app);
    let border_color = if app.chat.shell_focused {
        active_theme.text_muted
    } else {
        active_theme.primary
    };

    // Draw the vertical blue line on the left (separated by 1 column)
    let mut blue_line_lines = Vec::new();
    for _ in 0..input_height {
        blue_line_lines.push(Line::from(Span::styled(
            "┃",
            Style::default().fg(border_color),
        )));
    }
    frame.render_widget(
        Paragraph::new(blue_line_lines),
        Rect {
            x: flat_input_area.x,
            y: flat_input_area.y,
            width: 1,
            height: input_height,
        },
    );

    // The text block starts 1 column to the right of flat_input_area.x
    let text_block_area = Rect {
        x: flat_input_area.x.saturating_add(1),
        y: flat_input_area.y,
        width: flat_input_area.width.saturating_sub(1),
        height: input_height,
    };

    let active_theme = crate::tui::render::get_active_theme(app);
    let bg_color = active_theme
        .background_element
        .unwrap_or(Color::Rgb(24, 24, 24));
    let fg_color = active_theme.text;

    // Render background block
    let bg_block = Block::default().style(Style::default().bg(bg_color));
    frame.render_widget(bg_block, text_block_area);

    // Inside the text block, padding is 1.
    let inner_x = text_block_area.x.saturating_add(3);
    let inner_y = if app.screen == crate::app::Screen::AskUser {
        text_block_area.y
    } else {
        text_block_area.y.saturating_add(1)
    };
    let inner_w = if app.screen == crate::app::Screen::AskUser {
        text_block_area.width.saturating_sub(4)
    } else {
        text_block_area.width.saturating_sub(5) // match the new inner_x and leave 2 right padding
    };
    let inner_h = if app.screen == crate::app::Screen::AskUser {
        text_block_area.height
    } else {
        text_block_area.height.saturating_sub(1)
    };

    let text_area = if app.screen == crate::app::Screen::AskUser {
        Rect {
            x: inner_x,
            y: inner_y,
            width: inner_w,
            height: inner_h,
        }
    } else {
        Rect {
            x: inner_x,
            y: inner_y,
            width: inner_w,
            height: inner_h.saturating_sub(3), // leave 1 spacer + 1 mode/model + 1 bottom padding
        }
    };

    let mode_model_row_area = Rect {
        x: inner_x,
        y: inner_y.saturating_add(inner_h.saturating_sub(2)), // 1 spacer below input, 1 padding below indicator
        width: inner_w,
        height: 1,
    };

    let input_inner_width = text_area.width.max(1);
    let total_visual_lines = display_lines;

    // Compute cursor visual row and column
    let mut cursor_visual_row: u16 = 0;
    let mut cursor_col_in_logical: u16 = 0; // position within current logical line
    for (i, c) in app.chat.input.chars().enumerate() {
        if i == app.chat.cursor {
            break;
        }
        if c == '\n' {
            cursor_visual_row += 1;
            cursor_col_in_logical = 0;
        } else {
            cursor_col_in_logical += 1;
            if cursor_col_in_logical.is_multiple_of(input_inner_width) {
                cursor_visual_row += 1;
            }
        }
    }
    let cursor_x = cursor_col_in_logical % input_inner_width;

    // Auto-scroll: keep cursor visible within the scrollable viewport (text_area.height)
    let max_visible = text_area.height;
    let mut input_scroll = app.chat.input_scroll;
    if cursor_visual_row < input_scroll {
        input_scroll = cursor_visual_row;
    } else if cursor_visual_row >= input_scroll + max_visible {
        input_scroll = cursor_visual_row + 1 - max_visible;
    }
    let max_scroll = total_visual_lines.saturating_sub(max_visible);
    input_scroll = input_scroll.min(max_scroll);

    let paragraph_content: Vec<Line> = if app.chat.input.is_empty() {
        let placeholder = "Ask anything... \"Fix a TODO in the codebase\"";
        let active_theme = crate::tui::render::get_active_theme(app);
        let placeholder_color = active_theme.text_muted;
        vec![Line::from(Span::styled(
            placeholder,
            Style::default().fg(placeholder_color),
        ))]
    } else {
        wrapped_lines.into_iter().map(Line::from).collect()
    };

    if app.screen == crate::app::Screen::AskUser {
        render_ask_user_in_input_box(frame, app, text_block_area);
    } else if app.screen == crate::app::Screen::Permissions {
        render_permissions_in_input_box(frame, app, text_block_area);
    } else {
        // Render the scrollable paragraph
        frame.render_widget(
            Paragraph::new(paragraph_content)
                .style(Style::default().fg(fg_color))
                .scroll((input_scroll, 0)),
            text_area,
        );

        // Cursor position: text starts at text_area.x, y starts at text_area.y
        let cursor_y_in_box = cursor_visual_row.saturating_sub(input_scroll);
        let target_y = text_area.y + cursor_y_in_box;
        let max_y = text_area.bottom().saturating_sub(1);

        if !app.chat.shell_focused
            && !app.model_picker_open
            && target_y <= max_y
            && target_y >= text_area.y
        {
            frame.set_cursor_position((text_area.x + cursor_x, target_y));
        }

        // Render the mode/model indicator row inside the bottom area of the block
        let mode_color = match app.dev_mode {
            crate::app::DevelopMode::Plan => Color::Rgb(168, 85, 247), // purple
            crate::app::DevelopMode::Build => Color::Rgb(59, 130, 246), // blue (mockup)
        };

        let mode_str = app.dev_mode_label();
        let model_str = app.model_label();
        let mode_len = mode_str.chars().count() as u16;
        let model_len = model_str.chars().count() as u16;

        let mode_rect = Rect {
            x: mode_model_row_area.x,
            y: mode_model_row_area.y,
            width: mode_len,
            height: 1,
        };
        let model_rect = Rect {
            x: mode_model_row_area
                .x
                .saturating_add(mode_len)
                .saturating_add(3), // after mode label + separator " · "
            y: mode_model_row_area.y,
            width: model_len,
            height: 1,
        };
        app.chat.mode_area.set(Some(mode_rect));
        app.chat.model_area.set(Some(model_rect));

        let active_theme = crate::tui::render::get_active_theme(app);
        let separator_style = Style::default().fg(active_theme.text_muted);
        let mut mode_model_spans = vec![
            Span::styled(
                mode_str,
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", separator_style),
        ];

        // Split model label by whitespace: first two words theme text, subsequent words theme text_muted
        for (idx, word) in model_str.split_whitespace().enumerate() {
            if idx > 0 {
                mode_model_spans.push(Span::raw(" "));
            }
            let style = if idx < 2 {
                Style::default().fg(active_theme.text)
            } else {
                Style::default().fg(active_theme.text_muted)
            };
            mode_model_spans.push(Span::styled(word.to_owned(), style));
        }

        frame.render_widget(
            Paragraph::new(Line::from(mode_model_spans)).alignment(Alignment::Left),
            mode_model_row_area,
        );
    } // end else (normal input render)

    if let Some(pane) = right_pane {
        render_todos(frame, app, pane);
    }

    render_statusbar(frame, app, statusbar_area);

    let has_modal = app.model_picker_open
        || app.theme_picker_open
        || app.screen == crate::app::Screen::Sessions
        || app.screen == crate::app::Screen::Setup
        || app
            .pending
            .as_ref()
            .is_some_and(|p| matches!(p, crate::app::PendingTask::ConfirmFunction { .. }));

    if has_modal {
        dim_buffer(frame, area);
    }

    if app.model_picker_open {
        render_model_picker_modal(frame, app, area);
    }

    if app.theme_picker_open {
        render_theme_picker_modal(frame, app, area);
    }

    if app.screen == crate::app::Screen::Sessions {
        crate::tui::render::sessions::render_sessions_popup(frame, app, area);
    }

    if app.screen == crate::app::Screen::Setup {
        crate::tui::render::setup::render_setup_modal(frame, app, area);
    }

    if let Some(crate::app::PendingTask::ConfirmFunction { .. }) = &app.pending {
        render_confirm_modal(frame, app, area);
    }
} // end render_chat

fn dim_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            ((r as f32) * 0.35) as u8,
            ((g as f32) * 0.35) as u8,
            ((b as f32) * 0.35) as u8,
        ),
        Color::White => Color::Rgb(80, 80, 80),
        Color::Gray => Color::Rgb(50, 50, 50),
        Color::DarkGray => Color::Rgb(30, 30, 30),
        Color::Black => Color::Rgb(8, 8, 8),
        Color::Red => Color::Rgb(90, 0, 0),
        Color::Green => Color::Rgb(0, 90, 0),
        Color::Yellow => Color::Rgb(90, 90, 0),
        Color::Blue => Color::Rgb(0, 0, 90),
        Color::Magenta => Color::Rgb(90, 0, 90),
        Color::Cyan => Color::Rgb(0, 90, 90),
        c => c,
    }
}

fn dim_buffer(frame: &mut Frame, area: Rect) {
    let buffer = frame.buffer_mut();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = &mut buffer[(x, y)];
            cell.set_fg(dim_color(cell.fg));
            cell.set_bg(dim_color(cell.bg));
            cell.modifier.insert(Modifier::DIM);
        }
    }
}

fn render_confirm_modal(frame: &mut Frame, app: &App, area: Rect) {
    let Some(crate::app::PendingTask::ConfirmFunction { name, args }) = &app.pending else {
        return;
    };
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme
        .background_panel
        .unwrap_or(Color::Rgb(24, 24, 24));
    let modal_fg = active_theme.text;
    let dim_text = active_theme.text_muted;

    // Compact centered modal popup: 55% width, 45% height
    let popup_area = centered_rect(55, 45, area);
    frame.render_widget(ratatui::widgets::Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(modal_bg)),
        popup_area,
    );

    let margin = 1u16;
    let content = Rect {
        x: popup_area.x + margin,
        y: popup_area.y + margin,
        width: popup_area.width.saturating_sub(margin * 2),
        height: popup_area.height.saturating_sub(margin * 2),
    };
    if content.height == 0 || content.width == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Separator
            Constraint::Min(1),    // Body (diff)
            Constraint::Length(1), // Separator
            Constraint::Length(1), // Footer
        ])
        .split(content);

    // Title row (indented by 1)
    let title_area = Rect {
        x: chunks[0].x + 1,
        y: chunks[0].y,
        width: chunks[0].width.saturating_sub(2),
        height: 1,
    };
    let path_str = if name == "edit" || name == "write" || name == "patch" {
        args.get("path")
            .and_then(|v| v.as_str())
            .or_else(|| {
                args.get("edits")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|e| e.get("path"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("")
    } else {
        ""
    };

    let title_line = if path_str.is_empty() {
        Line::from(vec![
            Span::styled(
                "Confirm Tool Execution",
                Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  •  ", Style::default().fg(dim_text)),
            Span::styled(
                name.as_str(),
                Style::default()
                    .fg(Color::Rgb(245, 158, 11))
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                "Confirm Tool Execution",
                Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  •  ", Style::default().fg(dim_text)),
            Span::styled(
                name.as_str(),
                Style::default()
                    .fg(Color::Rgb(245, 158, 11))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(path_str, Style::default().fg(dim_text)),
        ])
    };
    frame.render_widget(Paragraph::new(title_line), title_area);

    // Header separator
    frame.render_widget(
        Paragraph::new("─".repeat(content.width as usize)).style(Style::default().fg(dim_text)),
        chunks[1],
    );

    // Body (indented by 1)
    let body_area = Rect {
        x: chunks[2].x + 1,
        y: chunks[2].y,
        width: chunks[2].width.saturating_sub(2),
        height: chunks[2].height,
    };
    if body_area.height > 0 {
        let mut body_lines: Vec<Line> = Vec::new();

        if name == "edit" {
            if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                for edit_val in edits.iter() {
                    let old_str = edit_val
                        .get("old_string")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let new_str = edit_val
                        .get("new_string")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    for line in old_str.lines() {
                        body_lines.push(Line::from(Span::styled(
                            format!("- {line}"),
                            Style::default()
                                .fg(Color::Rgb(255, 120, 120))
                                .bg(Color::Rgb(50, 15, 15)),
                        )));
                    }
                    for line in new_str.lines() {
                        body_lines.push(Line::from(Span::styled(
                            format!("+ {line}"),
                            Style::default()
                                .fg(Color::Rgb(120, 220, 120))
                                .bg(Color::Rgb(15, 45, 15)),
                        )));
                    }
                }
            } else {
                let old_str = args
                    .get("old_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new_str = args
                    .get("new_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                for line in old_str.lines() {
                    body_lines.push(Line::from(Span::styled(
                        format!("- {line}"),
                        Style::default()
                            .fg(Color::Rgb(255, 120, 120))
                            .bg(Color::Rgb(50, 15, 15)),
                    )));
                }
                for line in new_str.lines() {
                    body_lines.push(Line::from(Span::styled(
                        format!("+ {line}"),
                        Style::default()
                            .fg(Color::Rgb(120, 220, 120))
                            .bg(Color::Rgb(15, 45, 15)),
                    )));
                }
            }
        } else if name == "write" {
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            for line in content.lines() {
                body_lines.push(Line::from(Span::styled(
                    format!("+ {line}"),
                    Style::default()
                        .fg(Color::Rgb(120, 220, 120))
                        .bg(Color::Rgb(15, 45, 15)),
                )));
            }
        } else if name == "patch" {
            let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
            for line in patch.lines() {
                let style = if line.starts_with('+') && !line.starts_with("+++") {
                    Style::default()
                        .fg(Color::Rgb(120, 220, 120))
                        .bg(Color::Rgb(15, 45, 15))
                } else if line.starts_with('-') && !line.starts_with("---") {
                    Style::default()
                        .fg(Color::Rgb(255, 120, 120))
                        .bg(Color::Rgb(50, 15, 15))
                } else if line.starts_with("@@") {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(dim_text)
                };
                body_lines.push(Line::from(Span::styled(line.to_string(), style)));
            }
        } else {
            let args_str = serde_json::to_string_pretty(args).unwrap_or_default();
            for line in args_str.lines() {
                body_lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(modal_fg),
                )));
            }
        }

        let max_scroll = body_lines.len().saturating_sub(body_area.height as usize);
        let current_scroll = app.confirm_scroll.get().min(max_scroll as u16);
        app.confirm_scroll.set(current_scroll);

        frame.render_widget(
            Paragraph::new(body_lines.clone())
                .style(Style::default().fg(modal_fg))
                .wrap(Wrap { trim: true })
                .scroll((current_scroll, 0)),
            body_area,
        );

        if max_scroll > 0 {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("^"))
                .end_symbol(Some("v"))
                .track_symbol(Some("|"))
                .thumb_symbol("#");
            let mut scrollbar_state =
                ScrollbarState::new(body_lines.len()).position(current_scroll as usize);
            frame.render_stateful_widget(scrollbar, body_area, &mut scrollbar_state);
        }
    }

    // Footer separator
    frame.render_widget(
        Paragraph::new("─".repeat(content.width as usize)).style(Style::default().fg(dim_text)),
        chunks[3],
    );

    // Footer (indented by 1)
    let footer_area = Rect {
        x: chunks[4].x + 1,
        y: chunks[4].y,
        width: chunks[4].width.saturating_sub(2),
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "Y ",
                Style::default()
                    .fg(Color::Rgb(120, 220, 120))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("allow  ", Style::default().fg(dim_text)),
            Span::styled(
                "N ",
                Style::default()
                    .fg(Color::Rgb(255, 120, 120))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("deny", Style::default().fg(dim_text)),
        ])),
        footer_area,
    );
}

fn render_ask_user_in_input_box(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_fg = active_theme.text;
    let dim_text = active_theme.text_muted;
    let selected_color = active_theme.accent;

    let inner = Rect {
        x: area.x + 3,
        y: area.y + 1,
        width: area.width.saturating_sub(5),
        height: area.height.saturating_sub(1),
    };
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let q_wrapped = wrap_text_to_lines(app.ask_user.question.as_str(), inner.width as usize);
    let q_height = q_wrapped.len() as u16;

    // Render question
    for (i, line) in q_wrapped.iter().enumerate() {
        let row_y = inner.y + i as u16;
        if row_y >= inner.bottom() {
            break;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(line.as_str(), Style::default().fg(modal_fg))),
            Rect {
                x: inner.x,
                y: row_y,
                width: inner.width,
                height: 1,
            },
        );
    }

    let list_start_y = inner.y + q_height + 1;
    // footer 1 line from bottom (so there's 1 blank row below footer = bottom padding)
    let footer_y = inner.bottom().saturating_sub(2);
    let total_options = app.ask_user.options.len() + 1;
    let selected = app.ask_user.selected_idx;

    let mut row_y = list_start_y;
    for idx in 0..total_options {
        if row_y >= footer_y.saturating_sub(1) {
            break;
        } // leave 1 blank before footer
        let is_selected = idx == selected;
        let is_custom = idx == app.ask_user.options.len();
        let label: &str = if is_custom {
            "Type your own answer"
        } else {
            &app.ask_user.options[idx]
        };

        let num_style = if is_selected {
            Style::default()
                .fg(selected_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(dim_text)
        };
        let label_style = if is_selected {
            Style::default()
                .fg(selected_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(modal_fg).add_modifier(Modifier::BOLD)
        };

        // Label only — no description line
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{}.", idx + 1), num_style),
                Span::styled("  ", Style::default()),
                Span::styled(label, label_style),
            ])),
            Rect {
                x: inner.x,
                y: row_y,
                width: inner.width,
                height: 1,
            },
        );
        row_y += 1;

        // Custom text input (only for "Type your own answer" row when active)
        // Indent matches prefix of option line (e.g. "4.  ")
        if is_custom && app.ask_user.is_custom && row_y < footer_y.saturating_sub(1) {
            let prefix_len = format!("{}.  ", idx + 1).chars().count();
            let indent = " ".repeat(prefix_len);
            let display = if app.ask_user.custom_input.is_empty() {
                format!("{}Type your answer...", indent)
            } else {
                format!("{}{}", indent, app.ask_user.custom_input)
            };
            let input_style = if app.ask_user.custom_input.is_empty() {
                Style::default().fg(dim_text)
            } else {
                Style::default()
                    .fg(selected_color)
                    .add_modifier(Modifier::BOLD)
            };
            frame.render_widget(
                Paragraph::new(Span::styled(display, input_style)),
                Rect {
                    x: inner.x,
                    y: row_y,
                    width: inner.width,
                    height: 1,
                },
            );
            let cx = inner.x
                + prefix_len as u16
                + if app.ask_user.custom_input.is_empty() {
                    0
                } else {
                    app.ask_user.custom_input.chars().count() as u16
                };
            if cx < inner.right() {
                frame.set_cursor_position((cx, row_y));
            }
            row_y += 1;
        }
    }

    // Footer (with 1 blank line above and 1 below via footer_y offset)
    if footer_y < inner.bottom() {
        let footer = if app.ask_user.is_custom {
            Line::from(vec![
                Span::styled(
                    "Enter ",
                    Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("submit  ", Style::default().fg(dim_text)),
                Span::styled(
                    "Esc ",
                    Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("dismiss", Style::default().fg(dim_text)),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    "Up/Down ",
                    Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("select  ", Style::default().fg(dim_text)),
                Span::styled(
                    "Enter ",
                    Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("submit  ", Style::default().fg(dim_text)),
                Span::styled(
                    "Esc ",
                    Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("dismiss", Style::default().fg(dim_text)),
            ])
        };
        frame.render_widget(
            Paragraph::new(footer),
            Rect {
                x: inner.x,
                y: footer_y,
                width: inner.width,
                height: 1,
            },
        );
    }
}

fn render_permissions_in_input_box(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_fg = active_theme.text;
    let dim_text = active_theme.text_muted;
    let selected_color = active_theme.accent;

    let inner = Rect {
        x: area.x + 3,
        y: area.y + 1,
        width: area.width.saturating_sub(5),
        height: area.height.saturating_sub(1),
    };
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Title
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Select permission level",
            Style::default().fg(modal_fg),
        )),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );

    let options = crate::app::PermissionPickerState::options();
    let total = options.len();
    let selected = app.permissions.selected.min(total.saturating_sub(1));

    let footer_y = inner.bottom().saturating_sub(2);

    let mut row_y = inner.y + 2; // title(1) + blank(1)
    for (idx, &(label, desc, _)) in options.iter().enumerate().take(total) {
        let is_selected = idx == selected;

        let num_style = if is_selected {
            Style::default()
                .fg(selected_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(dim_text)
        };
        let label_style = if is_selected {
            Style::default()
                .fg(selected_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(modal_fg).add_modifier(Modifier::BOLD)
        };

        if row_y < footer_y.saturating_sub(1) {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{}.", idx + 1), num_style),
                    Span::styled("  ", Style::default()),
                    Span::styled(label, label_style),
                ])),
                Rect {
                    x: inner.x,
                    y: row_y,
                    width: inner.width,
                    height: 1,
                },
            );
            row_y += 1;
        }

        if row_y < footer_y.saturating_sub(1) {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(desc, Style::default().fg(dim_text)),
                ])),
                Rect {
                    x: inner.x,
                    y: row_y,
                    width: inner.width,
                    height: 1,
                },
            );
            row_y += 1;
        }
    }

    // Footer: 1 blank row below (bottom padding), so footer_y is 2 rows above inner.bottom()
    if footer_y < inner.bottom() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Up/Down ",
                    Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("select  ", Style::default().fg(dim_text)),
                Span::styled(
                    "Enter ",
                    Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("apply  ", Style::default().fg(dim_text)),
                Span::styled(
                    "Esc ",
                    Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled("cancel", Style::default().fg(dim_text)),
            ])),
            Rect {
                x: inner.x,
                y: footer_y,
                width: inner.width,
                height: 1,
            },
        );
    }
}

fn render_model_picker_modal(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme
        .background_panel
        .unwrap_or(Color::Rgb(24, 24, 24));
    let modal_fg = active_theme.text;
    let placeholder_fg = active_theme.text_muted;
    let list_fg = active_theme.text;
    let hint_fg = active_theme.text_muted;
    let active_bullet_color = active_theme.success;
    let active_bullet_color_on_select = active_theme.secondary;
    let select_bg = active_theme.accent;

    let popup_area = centered_rect(36, 48, area);
    frame.render_widget(ratatui::widgets::Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(modal_bg)),
        popup_area,
    );

    let margin = 1u16;
    let content_area = Rect {
        x: popup_area.x + margin,
        y: popup_area.y + margin,
        width: popup_area.width.saturating_sub(margin * 2),
        height: popup_area.height.saturating_sub(margin * 2),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(content_area);

    let title_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(chunks[0]);
    let title_content = title_row[1];

    frame.render_widget(
        Paragraph::new(Span::styled(
            "Select model",
            Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
        )),
        title_content,
    );
    frame.render_widget(
        Paragraph::new(Span::styled("esc", Style::default().fg(hint_fg)))
            .alignment(Alignment::Right),
        title_content,
    );

    let search_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(chunks[2]);
    let search_content = search_row[1];

    let search_line = if app.models.query.is_empty() {
        Line::from(Span::styled("Search", Style::default().fg(placeholder_fg)))
    } else {
        Line::from(Span::styled(
            app.models.query.clone(),
            Style::default().fg(modal_fg),
        ))
    };
    frame.render_widget(Paragraph::new(search_line), search_content);

    let cursor_x = search_content.x + app.models.query.chars().count() as u16;
    if cursor_x < search_content.right() {
        frame.set_cursor_position((cursor_x, search_content.y));
    }

    let list_area = chunks[4];
    let filtered = app.models.filtered_indices();
    let active_model = app.chat.config.model.as_str();

    if filtered.is_empty() {
        let msg = if app.models.query.is_empty() {
            "No models available"
        } else {
            "No matches"
        };
        let empty_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(list_area);
        frame.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(placeholder_fg))),
            empty_cols[1],
        );
        return;
    }

    let viewport = list_area.height as usize;
    let total = filtered.len();
    let selected = app.models.selected.min(total.saturating_sub(1));
    let start = if total <= viewport || selected < viewport / 2 {
        0
    } else if selected >= total - viewport / 2 {
        total - viewport
    } else {
        selected - viewport / 2
    };

    let visible_count = viewport.min(total.saturating_sub(start));

    for offset in 0..visible_count {
        let idx = start + offset;
        let model = &app.models.models[filtered[idx]];
        let display = model.trim_start_matches("models/");
        let is_selected = idx == selected;
        let is_active = display == active_model || model == active_model;

        let row_y = list_area.y + offset as u16;
        if row_y >= list_area.bottom() {
            break;
        }
        let row_area = Rect {
            x: list_area.x,
            y: row_y,
            width: list_area.width,
            height: 1,
        };

        if is_selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(select_bg)),
                row_area,
            );
        }

        let row_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(row_area);

        let bullet_style = if is_active {
            if is_selected {
                Style::default()
                    .bg(select_bg)
                    .fg(active_bullet_color_on_select)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(active_bullet_color)
                    .add_modifier(Modifier::BOLD)
            }
        } else if is_selected {
            Style::default().bg(select_bg)
        } else {
            Style::default()
        };

        let bullet_char = if is_active { "*" } else { " " };
        frame.render_widget(
            Paragraph::new(Span::styled(bullet_char, bullet_style)),
            row_cols[1],
        );

        let text_style = if is_selected {
            Style::default()
                .bg(select_bg)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(list_fg)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(display, text_style)),
            row_cols[3],
        );
    }
}

fn render_theme_picker_modal(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme
        .background_panel
        .unwrap_or(Color::Rgb(24, 24, 24));
    let modal_fg = active_theme.text;
    let placeholder_fg = active_theme.text_muted;
    let list_fg = active_theme.text;
    let hint_fg = active_theme.text_muted;
    let active_bullet_color = active_theme.success;
    let active_bullet_color_on_select = active_theme.secondary;
    let select_bg = active_theme.accent;

    let popup_area = centered_rect(36, 48, area);
    frame.render_widget(ratatui::widgets::Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(modal_bg)),
        popup_area,
    );

    let margin = 1u16;
    let content_area = Rect {
        x: popup_area.x + margin,
        y: popup_area.y + margin,
        width: popup_area.width.saturating_sub(margin * 2),
        height: popup_area.height.saturating_sub(margin * 2),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(content_area);

    let title_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(chunks[0]);
    let title_content = title_row[1];

    frame.render_widget(
        Paragraph::new(Span::styled(
            "Select theme",
            Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
        )),
        title_content,
    );
    frame.render_widget(
        Paragraph::new(Span::styled("esc", Style::default().fg(hint_fg)))
            .alignment(Alignment::Right),
        title_content,
    );

    let search_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(chunks[2]);
    let search_content = search_row[1];

    let search_line = if app.theme_picker.query.is_empty() {
        Line::from(Span::styled("Search", Style::default().fg(placeholder_fg)))
    } else {
        Line::from(Span::styled(
            app.theme_picker.query.clone(),
            Style::default().fg(modal_fg),
        ))
    };
    frame.render_widget(Paragraph::new(search_line), search_content);

    let cursor_x = search_content.x + app.theme_picker.query.chars().count() as u16;
    if cursor_x < search_content.right() {
        frame.set_cursor_position((cursor_x, search_content.y));
    }

    let list_area = chunks[4];
    let filtered = app.theme_picker.filtered_indices();

    if filtered.is_empty() {
        let msg = if app.theme_picker.query.is_empty() {
            "No themes available"
        } else {
            "No matches"
        };
        let empty_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(list_area);
        frame.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(placeholder_fg))),
            empty_cols[1],
        );
        return;
    }

    let viewport = list_area.height as usize;
    let total = filtered.len();
    let selected = app.theme_picker.selected.min(total.saturating_sub(1));
    let start = if total <= viewport || selected < viewport / 2 {
        0
    } else if selected >= total - viewport / 2 {
        total - viewport
    } else {
        selected - viewport / 2
    };

    let visible_count = viewport.min(total.saturating_sub(start));

    for offset in 0..visible_count {
        let idx = start + offset;
        let theme = &app.theme_picker.themes[filtered[idx]];
        let display = theme.label();
        let is_selected = idx == selected;
        let is_active = theme == &app.chat.config.theme;

        let row_y = list_area.y + offset as u16;
        if row_y >= list_area.bottom() {
            break;
        }
        let row_area = Rect {
            x: list_area.x,
            y: row_y,
            width: list_area.width,
            height: 1,
        };

        if is_selected {
            frame.render_widget(
                Block::default().style(Style::default().bg(select_bg)),
                row_area,
            );
        }

        let row_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(row_area);

        let bullet_style = if is_active {
            if is_selected {
                Style::default()
                    .bg(select_bg)
                    .fg(active_bullet_color_on_select)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(active_bullet_color)
                    .add_modifier(Modifier::BOLD)
            }
        } else if is_selected {
            Style::default().bg(select_bg)
        } else {
            Style::default()
        };

        let bullet_char = if is_active { "*" } else { " " };
        frame.render_widget(
            Paragraph::new(Span::styled(bullet_char, bullet_style)),
            row_cols[1],
        );

        let text_style = if is_selected {
            Style::default()
                .bg(select_bg)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(list_fg)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(display, text_style)),
            row_cols[3],
        );
    }
}

pub(crate) fn render_messages(frame: &mut Frame, app: &App, area: Rect) {
    let shell_focused = app.chat.shell_focused;
    if app.chat.messages.is_empty() {
        app.chat.messages_area.set(Some(area));
        *app.chat.message_line_ranges.borrow_mut() = Vec::new();
        let active_theme = crate::tui::render::get_active_theme(app);
        let lines = welcome_lines(Style::default().fg(active_theme.primary), area);
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }

    let mut all_lines = Vec::new();
    all_lines.push(Line::from("")); // Prepend an empty line for a top margin so messages don't glue to the top border

    let width = area.width.saturating_sub(6).max(1);

    let push_margin = |lines: &mut Vec<Line<'static>>| {
        if let Some(last) = lines.last()
            && (!last.spans.is_empty() || last.width() > 0)
        {
            lines.push(Line::from(""));
        }
    };

    let last_shell_idx = app.chat.messages.iter().rposition(|m| m.is_shell);
    let mut prev_is_tool = false;
    let mut message_line_ranges = Vec::new();

    for (msg_idx, message) in app.chat.messages.iter().enumerate() {
        let mut content = if message.pending {
            app.busy_label().unwrap_or_else(|| "Working...".to_owned())
        } else {
            message.text.clone()
        };

        let is_tool = message.is_tool;
        let is_shell = message.is_shell;

        // Strip "(empty)" prefix and any leading/trailing whitespace/newlines that follow it
        if !message.pending && !is_tool && !is_shell && message.author == "Darwin" {
            let trimmed = content.trim_start();
            if let Some(rest) = trimmed.strip_prefix("(empty)") {
                content = rest.trim_start().to_owned();
            }
        }

        // Skip completely empty assistant messages
        if !message.pending
            && !is_tool
            && !is_shell
            && message.author == "Darwin"
            && content.trim().is_empty()
        {
            continue;
        }

        let cached_ok = {
            let cache = message.cached_wrapped.borrow();
            if let Some((w, ref t, ref cached_lines)) = *cache {
                if w == width as usize && *t == get_theme(app) {
                    Some(cached_lines.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        let msg_lines = if let Some(lines) = cached_ok {
            lines
        } else {
            let mut msg_lines = Vec::new();
            if is_shell {
                let is_focused_shell = if shell_focused {
                    if let Some(ref active_id) = app.chat.focused_shell_session_id {
                        let last_of_session =
                            app.chat.messages.iter().enumerate().rposition(|(_, m)| {
                                m.is_shell && m.shell_session_id.as_ref() == Some(active_id)
                            });
                        Some(msg_idx) == last_of_session
                    } else if let Some(pid) = app.chat.focused_shell_pid {
                        message.shell_pid == Some(pid)
                    } else {
                        Some(msg_idx) == last_shell_idx
                    }
                } else {
                    false
                };

                let border_color = if is_focused_shell {
                    Color::Rgb(59, 130, 246)
                } else {
                    Color::DarkGray
                };

                let active_theme = crate::tui::render::get_active_theme(app);
                let bg_color = active_theme
                    .background_panel
                    .unwrap_or(Color::Rgb(28, 28, 28));
                let card_style = Style::default().bg(bg_color);

                let block_width = (area.width as usize).saturating_sub(5).max(1);

                let cmd_single_line = message.shell_cmd.replace(['\n', '\r'], " ");
                let max_title_cmd_len = block_width.saturating_sub(16); // e.g. "✗ Shell " is 8 chars, plus some margins/padding
                let cmd_vis_w = visual_width(&cmd_single_line);
                let cmd_summary = if cmd_vis_w > max_title_cmd_len {
                    let truncate_w = max_title_cmd_len.saturating_sub(3);
                    let (truncated, _) =
                        clean_and_truncate_to_visual_width(&cmd_single_line, truncate_w);
                    format!("{}...", truncated)
                } else {
                    let (cleaned, _) =
                        clean_and_truncate_to_visual_width(&cmd_single_line, max_title_cmd_len);
                    cleaned
                };
                let icon = if message.shell_success {
                    icons::SHELL_OK
                } else {
                    icons::SHELL_ERR
                };
                let title = format!("{icon} Shell {cmd_summary}");

                // Top padding line
                msg_lines.push(Line::from(vec![
                    Span::styled("┃", Style::default().fg(border_color)),
                    Span::styled(" ".repeat(block_width), card_style),
                ]));

                // Header line with command
                let (cleaned_title, title_vis_w) =
                    clean_and_truncate_to_visual_width(&title, block_width.saturating_sub(4));

                let title_fg = if is_focused_shell {
                    Color::Rgb(59, 130, 246)
                } else {
                    active_theme.text
                };
                let title_style = Style::default()
                    .fg(title_fg)
                    .bg(bg_color)
                    .add_modifier(Modifier::BOLD);

                let mut title_spans = vec![
                    Span::styled("┃", Style::default().fg(border_color)),
                    Span::styled("  ", card_style),
                    Span::styled(cleaned_title, title_style),
                ];
                let title_line_len = 2 + title_vis_w;
                let remaining = block_width.saturating_sub(title_line_len);
                if remaining > 0 {
                    title_spans.push(Span::styled(" ".repeat(remaining), card_style));
                }
                msg_lines.push(Line::from(title_spans));

                // Spacer line between command header and command output content if output exists
                let trimmed_content = content.trim_end();
                if !trimmed_content.is_empty() {
                    msg_lines.push(Line::from(vec![
                        Span::styled("┃", Style::default().fg(border_color)),
                        Span::styled(" ".repeat(block_width), card_style),
                    ]));

                    let body_w = block_width.saturating_sub(4);
                    let wrapped_body_lines = wrap_text_to_lines(trimmed_content, body_w);
                    let max_shell_lines = 25;
                    let (display_lines, _truncated_count) =
                        if wrapped_body_lines.len() > max_shell_lines {
                            let keep_start = 10;
                            let keep_end = 12;
                            let mut lines_to_show = Vec::new();
                            for l in wrapped_body_lines.iter().take(keep_start) {
                                lines_to_show.push(l.clone());
                            }
                            lines_to_show.push(format!(
                                "... [{} lines truncated] ...",
                                wrapped_body_lines.len() - keep_start - keep_end
                            ));
                            for l in wrapped_body_lines
                                .iter()
                                .skip(wrapped_body_lines.len() - keep_end)
                            {
                                lines_to_show.push(l.clone());
                            }
                            (
                                lines_to_show,
                                wrapped_body_lines.len() - keep_start - keep_end,
                            )
                        } else {
                            (wrapped_body_lines, 0)
                        };

                    for line in display_lines {
                        let is_truncation_marker =
                            line.starts_with("... [") && line.ends_with("] ...");
                        let (cleaned, line_vis_w) =
                            clean_and_truncate_to_visual_width(&line, body_w);
                        let text_style = if is_truncation_marker {
                            Style::default()
                                .fg(Color::Yellow)
                                .bg(bg_color)
                                .add_modifier(Modifier::ITALIC)
                        } else {
                            Style::default().fg(Color::DarkGray).bg(bg_color)
                        };

                        let mut body_spans = vec![
                            Span::styled("┃", Style::default().fg(border_color)),
                            Span::styled("  ", card_style),
                            Span::styled(cleaned, text_style),
                        ];
                        let line_len = 2 + line_vis_w;
                        let remaining = block_width.saturating_sub(line_len);
                        if remaining > 0 {
                            body_spans.push(Span::styled(" ".repeat(remaining), card_style));
                        }
                        msg_lines.push(Line::from(body_spans));
                    }
                }

                // Bottom padding line
                msg_lines.push(Line::from(vec![
                    Span::styled("┃", Style::default().fg(border_color)),
                    Span::styled(" ".repeat(block_width), card_style),
                ]));
            } else {
                let lines_count = content.lines().count();
                let limit = 500;
                let (display_content, is_truncated) = if lines_count > limit {
                    let truncated: String = content
                        .lines()
                        .take(limit)
                        .collect::<Vec<&str>>()
                        .join("\n");
                    (truncated, true)
                } else {
                    (content.clone(), false)
                };

                let parsed_lines = parse_markdown_lines(
                    &display_content,
                    &crate::tui::render::get_active_theme(app),
                );
                match message.author {
                    "You" => {
                        let mut wrapped_parsed_lines = wrap_lines(parsed_lines, width as usize);
                        if is_truncated {
                            wrapped_parsed_lines.push(Line::from(Span::styled(
                                format!("... [Message truncated: {} more lines. Use a paging tool or scroll inside editor to view full text.]", lines_count - limit),
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC)
                            )));
                        }

                        let active_theme = crate::tui::render::get_active_theme(app);
                        let user_bg = active_theme
                            .background_panel
                            .unwrap_or(Color::Rgb(24, 24, 24));
                        let user_fg = active_theme.text;
                        let user_style = Style::default().bg(user_bg).fg(user_fg);

                        let border_color = if app.chat.shell_focused {
                            Color::DarkGray
                        } else {
                            Color::Rgb(59, 130, 246)
                        };

                        // Subtract 1 column for the vertical highlight line "┃"
                        let block_width = (area.width as usize).saturating_sub(5).max(1);

                        // Top padding line
                        msg_lines.push(Line::from(vec![
                            Span::styled("┃", Style::default().fg(border_color)),
                            Span::styled(" ".repeat(block_width), user_style),
                        ]));

                        for line in wrapped_parsed_lines {
                            let mut spans = vec![
                                Span::styled("┃", Style::default().fg(border_color)),
                                Span::styled(" ", user_style), // 1 space left padding inside bubble
                            ];
                            let line_text_width = line.width();
                            for s in &line.spans {
                                let style = s.style.patch(user_style);
                                spans.push(s.clone().style(style));
                            }

                            let remaining = block_width.saturating_sub(line_text_width + 1);
                            if remaining > 0 {
                                spans.push(Span::styled(" ".repeat(remaining), user_style));
                            }
                            msg_lines.push(Line::from(spans));
                        }

                        // Bottom padding line
                        msg_lines.push(Line::from(vec![
                            Span::styled("┃", Style::default().fg(border_color)),
                            Span::styled(" ".repeat(block_width), user_style),
                        ]));
                    }
                    "System" => {
                        let mut wrapped_parsed_lines = wrap_lines(parsed_lines, width as usize);
                        if is_truncated {
                            wrapped_parsed_lines.push(Line::from(Span::styled(
                                format!("... [Message truncated: {} more lines. Use a paging tool or scroll inside editor to view full text.]", lines_count - limit),
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC)
                            )));
                        }

                        msg_lines.push(Line::from(vec![Span::styled(
                            "system error",
                            Style::default().add_modifier(Modifier::BOLD).fg(Color::Red),
                        )]));
                        for line in wrapped_parsed_lines {
                            let mut spans = Vec::new();
                            for span in line.spans {
                                spans.push(span.style(Style::default().fg(Color::Red)));
                            }
                            msg_lines.push(Line::from(spans));
                        }
                    }
                    _ => {
                        let darwin_width = (width as usize).saturating_sub(6).max(1);
                        let mut wrapped_parsed_lines = wrap_lines(parsed_lines, darwin_width);
                        if is_truncated {
                            wrapped_parsed_lines.push(Line::from(Span::styled(
                                format!("... [Message truncated: {} more lines. Use a paging tool or scroll inside editor to view full text.]", lines_count - limit),
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC)
                            )));
                        }

                        for line in wrapped_parsed_lines {
                            let mut spans = vec![Span::raw("    ")]; // 4 spaces left margin for assistant messages
                            spans.extend(line.spans);
                            msg_lines.push(Line::from(spans));
                        }
                    }
                }
            }
            *message.cached_wrapped.borrow_mut() =
                Some((width as usize, get_theme(app), msg_lines.clone()));
            msg_lines
        };

        if !all_lines.is_empty()
            && msg_idx > 0
            && (is_shell
                || message.author == "You"
                || message.author == "System"
                || !is_tool
                || !prev_is_tool)
        {
            push_margin(&mut all_lines);
        }

        let start_line = all_lines.len();
        all_lines.extend(msg_lines);
        let end_line = all_lines.len();
        message_line_ranges.push((msg_idx, start_line, end_line));
        prev_is_tool = is_tool;
    }

    app.chat.messages_area.set(Some(area));
    *app.chat.message_line_ranges.borrow_mut() = message_line_ranges;

    // Line-by-line scrolling
    let total_lines = all_lines.len();
    let viewport_height = area.height as usize;

    let max_scroll = total_lines.saturating_sub(viewport_height);
    let scroll_offset = (app.chat.scroll as usize).min(max_scroll);
    let scroll_y = max_scroll.saturating_sub(scroll_offset);

    let start_idx = scroll_y;
    let end_idx = (start_idx + viewport_height).min(total_lines);
    let visible_lines = all_lines[start_idx..end_idx].to_vec();

    let flat_messages_area = Rect {
        x: area.x.saturating_add(2),
        y: area.y,
        width: area.width.saturating_sub(4),
        height: area.height,
    };
    frame.render_widget(Paragraph::new(visible_lines), flat_messages_area);
}

fn render_command_suggestions(frame: &mut Frame, app: &App, area: Rect) {
    let suggestions = app.command_suggestions();
    if suggestions.is_empty() {
        return;
    }

    let active_theme = crate::tui::render::get_active_theme(app);
    let bg_color = active_theme
        .background_panel
        .unwrap_or(Color::Rgb(24, 24, 24));

    let border_color = if app.chat.shell_focused {
        Color::DarkGray
    } else {
        Color::Rgb(59, 130, 246)
    };

    let mut line_lines = Vec::new();
    for _ in 0..area.height {
        line_lines.push(Line::from(Span::styled(
            "┃",
            Style::default().fg(border_color),
        )));
    }
    frame.render_widget(
        Paragraph::new(line_lines),
        Rect {
            x: area.x,
            y: area.y,
            width: 1,
            height: area.height,
        },
    );

    let list_area = Rect {
        x: area.x.saturating_add(1),
        y: area.y,
        width: area.width.saturating_sub(1),
        height: area.height,
    };

    // Draw background for suggestions box to match prompt background color
    frame.render_widget(
        Block::default().style(Style::default().bg(bg_color)),
        list_area,
    );

    let total_len = suggestions.len();
    let window_size = 10;
    let selected_idx = app.chat.suggestion_idx.min(total_len.saturating_sub(1));

    let start_idx = if total_len <= window_size || selected_idx < window_size / 2 {
        0
    } else if selected_idx >= total_len - window_size / 2 {
        total_len - window_size
    } else {
        selected_idx - window_size / 2
    };

    let visible_suggestions: Vec<_> = suggestions
        .into_iter()
        .skip(start_idx)
        .take(window_size)
        .collect();

    let max_name_len = visible_suggestions
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(0);

    let mut items = Vec::new();
    for (idx, suggestion) in visible_suggestions.into_iter().enumerate() {
        let global_idx = start_idx + idx;
        let is_active = global_idx == selected_idx;
        let name_len = suggestion.name.chars().count();
        let padding_spaces = " ".repeat(max_name_len.saturating_sub(name_len) + 4);

        let line = if is_active {
            Line::from(vec![
                Span::styled(
                    format!(" {}", suggestion.name),
                    Style::default()
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(padding_spaces, Style::default().fg(Color::Black)),
                Span::styled(
                    suggestion.description,
                    Style::default().fg(Color::Rgb(60, 60, 60)),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    format!(" {}", suggestion.name),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(padding_spaces, Style::default().fg(Color::DarkGray)),
                Span::styled(suggestion.description, Style::default().fg(Color::DarkGray)),
            ])
        };

        let item_style = if is_active {
            Style::default()
                .bg(Color::Rgb(134, 194, 172))
                .fg(Color::Black)
        } else {
            Style::default()
        };

        items.push(ListItem::new(line).style(item_style));
    }

    let fg_color = active_theme.text;
    let block = Block::default();
    let list_widget = List::new(items)
        .block(block)
        .style(Style::default().bg(bg_color).fg(fg_color));
    frame.render_widget(list_widget, list_area);
}

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn visual_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    let mut w = 0;
    for c in s.chars() {
        if c == '\t' {
            w += 4;
        } else if c == '\r' || c == '\n' {
            // ignore
        } else if !c.is_control() {
            w += c.width().unwrap_or(0);
        }
    }
    w
}

fn clean_and_truncate_to_visual_width(s: &str, max_w: usize) -> (String, usize) {
    use unicode_width::UnicodeWidthChar;
    let mut res = String::new();
    let mut current_w = 0;
    for c in s.chars() {
        if c == '\r' || c == '\n' {
            continue;
        }
        let char_w = if c == '\t' {
            4
        } else if c.is_control() {
            0
        } else {
            c.width().unwrap_or(0)
        };
        if current_w + char_w > max_w {
            break;
        }
        if c == '\t' {
            res.push_str("    ");
        } else if !c.is_control() {
            res.push(c);
        }
        current_w += char_w;
    }
    (res, current_w)
}

fn render_todos(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let sidebar_bg = active_theme
        .background_panel
        .unwrap_or(Color::Rgb(24, 24, 24));
    let sidebar_fg = active_theme.text;

    let block = Block::default()
        .style(Style::default().bg(sidebar_bg).fg(sidebar_fg))
        .padding(Padding::new(1, 1, 1, 1));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area to center the title "TODO" horizontally
    let todo_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title "TODO"
            Constraint::Length(1), // Spacer
            Constraint::Min(1),    // Tasks
        ])
        .split(inner_area);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "TODO",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        todo_chunks[0],
    );

    let mut lines = Vec::new();

    let render_priority = |p: &str| -> Span<'static> {
        match p {
            "high" => Span::styled("!! ", Style::default().fg(Color::Rgb(239, 68, 68))),
            "medium" => Span::styled("!  ", Style::default().fg(Color::Rgb(245, 158, 11))),
            "low" => Span::styled("   ", Style::default()),
            _ => Span::raw("   "),
        }
    };

    let in_progress: Vec<&TodoItem> = app
        .chat
        .todos
        .iter()
        .filter(|t| t.status == "in_progress")
        .collect();
    let pending: Vec<&TodoItem> = app
        .chat
        .todos
        .iter()
        .filter(|t| t.status == "pending")
        .collect();
    let completed: Vec<&TodoItem> = app
        .chat
        .todos
        .iter()
        .filter(|t| t.status == "completed")
        .collect();
    let cancelled: Vec<&TodoItem> = app
        .chat
        .todos
        .iter()
        .filter(|t| t.status == "cancelled")
        .collect();

    let mut all_sorted_todos = Vec::new();
    all_sorted_todos.extend(in_progress);
    all_sorted_todos.extend(pending);
    all_sorted_todos.extend(completed);
    all_sorted_todos.extend(cancelled);

    let tasks_area = todo_chunks[2];

    for item in all_sorted_todos {
        let (status_bullet, item_style) = match item.status.as_str() {
            "in_progress" => (
                Span::styled("● ", Style::default().fg(Color::Rgb(234, 179, 8))),
                Style::default().fg(Color::Rgb(220, 220, 220)),
            ),
            "completed" => (
                Span::styled("● ", Style::default().fg(Color::Rgb(34, 197, 94))),
                Style::default().fg(Color::DarkGray),
            ),
            "cancelled" => (
                Span::styled("◌ ", Style::default().fg(Color::DarkGray)),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
            _ => (
                Span::styled("○ ", Style::default().fg(Color::DarkGray)),
                Style::default().fg(Color::DarkGray),
            ),
        };

        let priority_marker = render_priority(&item.priority);
        let content_width = (tasks_area.width as usize).saturating_sub(6).max(1);
        let wrapped = wrap_text_to_lines(&item.content, content_width);
        for (idx, line_text) in wrapped.iter().enumerate() {
            if idx == 0 {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    status_bullet.clone(),
                    priority_marker.clone(),
                    Span::styled(line_text.clone(), item_style),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(line_text.clone(), item_style),
                ]));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), tasks_area);
}
