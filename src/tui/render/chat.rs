use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Padding, Paragraph, Wrap};

use crate::app::{App, PendingTask};
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
        suggestions.len().min(3) as u16 + 2
    };

    // Max 6 lines for input (border=2, padding=2, so inner width = area.width - 4)
    let input_inner_w = if app.chat.messages.is_empty() {
        // Centered welcome input box is 70% of screen width
        ((area.width as u32 * 70 / 100) as u16)
            .saturating_sub(4)
            .max(1)
    } else {
        area.width.saturating_sub(4).max(1)
    };
    let wrapped_lines = wrap_text_to_lines(app.chat.input.as_str(), input_inner_w as usize);
    let display_lines = wrapped_lines.len() as u16;
    let input_height = display_lines.clamp(1, 5) + 2;

    let (
        messages_area,
        suggestions_area,
        queue_area,
        input_area,
        statusbar_area,
        logo_area,
        tips_area,
    ) = if app.chat.messages.is_empty() {
        let logo_lines = logo_lines();
        let logo_fits_flag = logo_fits(&logo_lines, area.width, 5);

        let empty_chunks = if logo_fits_flag {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(25), // Top space (centers the logo vertically)
                    Constraint::Length(6),      // Logo (5 lines + 1 spacer)
                    Constraint::Length(input_height), // Input box
                    Constraint::Length(2),      // Spacer
                    Constraint::Length(1),      // Tips line
                    Constraint::Min(0),         // Bottom space
                    Constraint::Length(1),      // Status bar
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),               // Top space (centers the input box vertically)
                    Constraint::Length(input_height), // Input box
                    Constraint::Length(2),            // Spacer
                    Constraint::Length(1),            // Tips line
                    Constraint::Min(0),               // Bottom space
                    Constraint::Length(1),            // Status bar
                ])
                .split(area)
        };

        let input_chunk = if logo_fits_flag {
            empty_chunks[2]
        } else {
            empty_chunks[1]
        };
        let centered_input_box = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(15),
                Constraint::Percentage(70),
                Constraint::Percentage(15),
            ])
            .split(input_chunk)[1];

        if logo_fits_flag {
            (
                None,
                None,
                None,
                Some(centered_input_box),
                empty_chunks[6],
                Some(empty_chunks[1]),
                Some(empty_chunks[4]),
            )
        } else {
            (
                None,
                None,
                None,
                Some(centered_input_box),
                empty_chunks[5],
                None,
                Some(empty_chunks[3]),
            )
        }
    } else {
        let queue_height = if app.chat.message_queue.is_empty() {
            0
        } else {
            (app.chat.message_queue.len().min(3) as u16) + 1
        };

        let normal_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(4),
                Constraint::Length(suggestion_height),
                Constraint::Length(queue_height),
                Constraint::Length(input_height),
                Constraint::Length(1),
            ])
            .split(area);

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
            Some(normal_chunks[3]),
            normal_chunks[4],
            None,
            None,
        )
    };

    if let Some(logo_area) = logo_area {
        let logo = logo_lines_for_area(logo_area.width, 5);
        frame.render_widget(Paragraph::new(logo).alignment(Alignment::Center), logo_area);
    }

    if let Some(tips_area) = tips_area {
        let tips_line = Line::from(vec![
            Span::styled(
                "Ctrl+S",
                Style::default()
                    .fg(Color::Rgb(236, 72, 153))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Setup  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Ctrl+P",
                Style::default()
                    .fg(Color::Rgb(168, 85, 247))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Model  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "/help",
                Style::default()
                    .fg(Color::Rgb(59, 130, 246))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Help", Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(
            Paragraph::new(tips_line).alignment(Alignment::Center),
            tips_area,
        );
    }

    if let Some(messages_area) = messages_area {
        render_messages(frame, app, messages_area);
    }

    if let Some(suggestions_area) = suggestions_area {
        render_command_suggestions(frame, app, suggestions_area);
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
    // inner content width: border(1 each side) + padding(1 each side) = -4 total
    let input_inner_width = input_box.width.saturating_sub(4).max(1);
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

    // Auto-scroll: keep cursor visible within the 5-row viewport (rows 0..4 inside box)
    let max_visible: u16 = (input_height - 2).min(5); // rows inside border
    let mut input_scroll = app.chat.input_scroll;
    if cursor_visual_row < input_scroll {
        input_scroll = cursor_visual_row;
    } else if cursor_visual_row >= input_scroll + max_visible {
        input_scroll = cursor_visual_row + 1 - max_visible;
    }
    let max_scroll = total_visual_lines.saturating_sub(max_visible);
    input_scroll = input_scroll.min(max_scroll);

    let paragraph_content: Vec<Line> = wrapped_lines.into_iter().map(Line::from).collect();

    let border_style = if app.chat.shell_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };

    frame.render_widget(
        Paragraph::new(paragraph_content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style)
                    .title(" Message (Alt+Enter: newline) ")
                    .padding(Padding::horizontal(1)),
            )
            .scroll((input_scroll, 0)),
        input_box,
    );

    // Cursor position: border(1) + padding(1) = offset 2 on x; border(1) on y
    let cursor_y_in_box = cursor_visual_row.saturating_sub(input_scroll);
    let target_y = input_box.y + 1 + cursor_y_in_box;
    let max_y = input_box.bottom().saturating_sub(2);

    if !app.chat.shell_focused && target_y <= max_y && target_y > input_box.y {
        frame.set_cursor_position((input_box.x + 2 + cursor_x, target_y));
    }

    render_statusbar(frame, app, statusbar_area);

    if let Some(PendingTask::ConfirmFunction { name, args }) = &app.pending {
        let popup_area = if name == "edit_file" || name == "edit_files" || name == "write_file" {
            centered_rect(80, 70, area)
        } else {
            centered_rect(60, 50, area)
        };
        let inner_width = (popup_area.width as usize).saturating_sub(4);

        let mut text = vec![
            Line::from(vec![
                Span::styled("Tool: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(name),
            ]),
            Line::from(""),
        ];

        if name == "edit_file" {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old_str = args
                .get("old_string")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let new_str = args
                .get("new_string")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            text.push(Line::from(vec![
                Span::styled("File: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(path, Style::default().fg(Color::Cyan)),
            ]));
            text.push(Line::from(""));
            text.push(Line::from(Span::styled(
                "Diff Preview:",
                Style::default().add_modifier(Modifier::BOLD),
            )));

            for line in old_str.lines() {
                let formatted = format!("- {line}");
                let padded = format!("{formatted:<width$}", width = inner_width);
                text.push(Line::from(Span::styled(
                    padded,
                    Style::default()
                        .fg(Color::Rgb(255, 180, 180))
                        .bg(Color::Rgb(70, 20, 20)),
                )));
            }
            for line in new_str.lines() {
                let formatted = format!("+ {line}");
                let padded = format!("{formatted:<width$}", width = inner_width);
                text.push(Line::from(Span::styled(
                    padded,
                    Style::default()
                        .fg(Color::Rgb(180, 255, 180))
                        .bg(Color::Rgb(20, 60, 20)),
                )));
            }
            text.push(Line::from(""));
        } else if name == "edit_files" {
            if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                for (idx, edit_val) in edits.iter().enumerate() {
                    let path = edit_val.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let old_str = edit_val
                        .get("old_string")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let new_str = edit_val
                        .get("new_string")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    text.push(Line::from(vec![
                        Span::styled(
                            format!("Edit #{} (", idx + 1),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(path, Style::default().fg(Color::Cyan)),
                        Span::styled("):", Style::default().add_modifier(Modifier::BOLD)),
                    ]));

                    for line in old_str.lines() {
                        let formatted = format!("- {line}");
                        let padded = format!("{formatted:<width$}", width = inner_width);
                        text.push(Line::from(Span::styled(
                            padded,
                            Style::default()
                                .fg(Color::Rgb(255, 180, 180))
                                .bg(Color::Rgb(70, 20, 20)),
                        )));
                    }
                    for line in new_str.lines() {
                        let formatted = format!("+ {line}");
                        let padded = format!("{formatted:<width$}", width = inner_width);
                        text.push(Line::from(Span::styled(
                            padded,
                            Style::default()
                                .fg(Color::Rgb(180, 255, 180))
                                .bg(Color::Rgb(20, 60, 20)),
                        )));
                    }
                    text.push(Line::from(""));
                }
            }
        } else if name == "write_file" {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

            text.push(Line::from(vec![
                Span::styled("File: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(path, Style::default().fg(Color::Cyan)),
            ]));
            text.push(Line::from(""));
            text.push(Line::from(Span::styled(
                "Content to Write:",
                Style::default().add_modifier(Modifier::BOLD),
            )));

            for line in content.lines() {
                let formatted = format!("+ {line}");
                let padded = format!("{formatted:<width$}", width = inner_width);
                text.push(Line::from(Span::styled(
                    padded,
                    Style::default()
                        .fg(Color::Rgb(180, 255, 180))
                        .bg(Color::Rgb(20, 60, 20)),
                )));
            }
            text.push(Line::from(""));
        } else {
            text.push(Line::from(vec![
                Span::styled("Args: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(serde_json::to_string_pretty(args).unwrap_or_default()),
            ]));
            text.push(Line::from(""));
        }

        text.push(Line::from(vec![
            Span::styled(
                "[Y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Allow   "),
            Span::styled(
                "[N]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Deny"),
        ]));

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(
            Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" Tool Access Request (↑/↓ to scroll) "),
                )
                .wrap(Wrap { trim: true })
                .scroll((app.confirm_scroll, 0)),
            popup_area,
        );
    }
}

pub(crate) fn render_messages(frame: &mut Frame, app: &App, area: Rect) {
    let shell_focused = app.chat.shell_focused;
    let block_border_style = if shell_focused {
        Style::default()
            .fg(Color::Rgb(59, 130, 246))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = if shell_focused {
        " 󰍡 Conversations (Tab to return) "
    } else {
        " 󰍡 Conversations "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(block_border_style)
        .title(title)
        .padding(Padding::horizontal(1));

    if app.chat.messages.is_empty() {
        let welcome_area = block.inner(area);
        let lines = welcome_lines(welcome_area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            welcome_area,
        );
        return;
    }

    let inner_area = block.inner(area);
    let mut all_lines = Vec::new();
    let width = inner_area.width.saturating_sub(4);

    let push_margin = |lines: &mut Vec<Line<'static>>| {
        if let Some(last) = lines.last()
            && (!last.spans.is_empty() || last.width() > 0)
        {
            lines.push(Line::from(""));
        }
    };

    let last_shell_idx = app.chat.messages.iter().rposition(|m| m.is_shell);
    let mut prev_is_tool = false;

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
            if let Some((w, t, ref cached_lines)) = *cache {
                if w == width as usize && t == get_theme(app) && !is_shell {
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
                let is_last_shell = Some(msg_idx) == last_shell_idx;
                let border_color = if shell_focused && is_last_shell {
                    Color::Rgb(59, 130, 246)
                } else {
                    Color::DarkGray
                };
                let border_style = Style::default().fg(border_color);

                let title_style = if shell_focused && is_last_shell {
                    Style::default()
                        .fg(Color::Rgb(59, 130, 246))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                };

                let cmd_single_line = message.shell_cmd.replace(['\n', '\r'], " ");
                let title_w = (width as usize).saturating_sub(1);
                let max_title_cmd_len = title_w.saturating_sub(12); // "✗ Shell " is 8 chars, some margin
                let cmd_summary = if cmd_single_line.chars().count() > max_title_cmd_len {
                    let truncated: String = cmd_single_line
                        .chars()
                        .take(max_title_cmd_len.saturating_sub(3))
                        .collect();
                    format!("{}...", truncated)
                } else {
                    cmd_single_line
                };
                let icon = if message.shell_success {
                    icons::SHELL_OK
                } else {
                    icons::SHELL_ERR
                };
                let title = format!("{icon} Shell {cmd_summary}");

                msg_lines.push(Line::from(Span::styled(
                    format!("╭{}╮", "─".repeat(width as usize)),
                    border_style,
                )));

                let title_len = title.chars().count();
                let title_pad = title_w.saturating_sub(title_len);
                let padded_title = if title_len > title_w {
                    title.chars().take(title_w).collect()
                } else {
                    format!("{}{}", title, " ".repeat(title_pad))
                };

                msg_lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(padded_title, title_style),
                    Span::styled("│", border_style),
                ]));

                let body_w = (width as usize).saturating_sub(2);
                let wrapped_body_lines = wrap_text_to_lines(&content, body_w);
                for line in wrapped_body_lines {
                    let line_len = line.chars().count();
                    let body_pad = body_w.saturating_sub(line_len);
                    let padded_line = if line_len > body_w {
                        line.chars().take(body_w).collect()
                    } else {
                        format!("{}{}", line, " ".repeat(body_pad))
                    };

                    msg_lines.push(Line::from(vec![
                        Span::styled("│  ", border_style),
                        Span::styled(padded_line, Style::default().fg(Color::DarkGray)),
                        Span::styled("│", border_style),
                    ]));
                }
                msg_lines.push(Line::from(Span::styled(
                    format!("╰{}╯", "─".repeat(width as usize)),
                    border_style,
                )));
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

                let parsed_lines = parse_markdown_lines(&display_content);
                let mut wrapped_parsed_lines = wrap_lines(parsed_lines, width as usize);
                if is_truncated {
                    wrapped_parsed_lines.push(Line::from(Span::styled(
                        format!("... [Message truncated: {} more lines. Use a paging tool or scroll inside editor to view full text.]", lines_count - limit),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC)
                    )));
                }

                match message.author {
                    "You" => {
                        let theme = get_theme(app);
                        let (user_bg, user_fg) = match theme {
                            crate::config::Theme::Dark | crate::config::Theme::Auto => {
                                (Color::Rgb(255, 255, 255), Color::Rgb(0, 0, 0))
                            }
                            crate::config::Theme::Light => {
                                (Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255))
                            }
                        };
                        let user_style = Style::default().bg(user_bg).fg(user_fg);
                        let block_width = inner_area.width as usize;

                        msg_lines.push(Line::from(Span::styled(
                            " ".repeat(block_width),
                            user_style,
                        )));

                        for line in wrapped_parsed_lines {
                            let mut spans = vec![Span::styled("  ", user_style)];
                            let line_text_width = line.width();
                            for s in &line.spans {
                                let style = s.style.patch(user_style);
                                spans.push(s.clone().style(style));
                            }

                            let remaining = block_width.saturating_sub(line_text_width + 2);
                            if remaining > 0 {
                                spans.push(Span::styled(" ".repeat(remaining), user_style));
                            }
                            msg_lines.push(Line::from(spans));
                        }

                        msg_lines.push(Line::from(Span::styled(
                            " ".repeat(block_width),
                            user_style,
                        )));
                    }
                    "System" => {
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
                        for line in wrapped_parsed_lines {
                            let mut spans = vec![Span::raw("  ")];
                            spans.extend(line.spans);
                            msg_lines.push(Line::from(spans));
                        }
                    }
                }
            }
            if !is_shell {
                *message.cached_wrapped.borrow_mut() =
                    Some((width as usize, get_theme(app), msg_lines.clone()));
            }
            msg_lines
        };

        if !all_lines.is_empty()
            && (is_shell
                || message.author == "You"
                || message.author == "System"
                || !is_tool
                || !prev_is_tool)
        {
            push_margin(&mut all_lines);
        }

        all_lines.extend(msg_lines);
        prev_is_tool = is_tool;
    }

    // Line-by-line scrolling
    let total_lines = all_lines.len();
    let viewport_height = inner_area.height as usize;

    let max_scroll = total_lines.saturating_sub(viewport_height);
    let scroll_offset = (app.chat.scroll as usize).min(max_scroll);
    let scroll_y = max_scroll.saturating_sub(scroll_offset);

    let start_idx = scroll_y;
    let end_idx = (start_idx + viewport_height).min(total_lines);
    let visible_lines = all_lines[start_idx..end_idx].to_vec();

    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(visible_lines), inner_area);
}

fn render_command_suggestions(frame: &mut Frame, app: &App, area: Rect) {
    let items = app
        .command_suggestions()
        .into_iter()
        .take(3)
        .map(|suggestion| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    suggestion.name,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::raw(suggestion.description),
            ]))
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Commands ")
                .padding(Padding::horizontal(1)),
        ),
        area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
