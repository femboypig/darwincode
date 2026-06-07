use crate::app::App;
use crate::tui::render::get_active_theme;
use crate::tui::render::get_theme;
use crate::tui::render::icons::icons;
use crate::tui::render::logo::welcome_lines;
use crate::tui::syntax::{parse_markdown_lines, wrap_lines, wrap_text_to_lines};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

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

                let active_theme = crate::tui::render::get_active_theme(app);
                let border_color = if is_focused_shell {
                    Color::Rgb(59, 130, 246)
                } else {
                    active_theme.border
                };

                let bg_color = active_theme.background_panel.unwrap_or({
                    if active_theme.is_light {
                        Color::Rgb(240, 240, 240)
                    } else {
                        Color::Rgb(28, 28, 28)
                    }
                });
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
                            Style::default().fg(active_theme.text_muted).bg(bg_color)
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
                        let user_bg = active_theme.background_panel.unwrap_or({
                            if active_theme.is_light {
                                Color::Rgb(240, 240, 240)
                            } else {
                                Color::Rgb(24, 24, 24)
                            }
                        });
                        let user_fg = active_theme.text;
                        let user_style = Style::default().bg(user_bg).fg(user_fg);

                        let border_color = if app.chat.shell_focused {
                            active_theme.border
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
        let mut final_msg_lines = msg_lines;
        if let Some(ref sel) = app.chat.selection
            && sel.msg_idx == msg_idx
            && message.author == "Darwin"
            && !message.is_shell
            && !message.is_tool
        {
            let (min_line, min_col, max_line, max_col) = sel.normalized();
            let highlight_bg = get_active_theme(app).accent;
            let highlight_fg = if get_active_theme(app).is_light {
                Color::Black
            } else {
                Color::White
            };
            let highlight_style = Style::default().bg(highlight_bg).fg(highlight_fg);
            for (line_idx, line) in final_msg_lines.iter_mut().enumerate() {
                if line_idx >= min_line && line_idx <= max_line {
                    let text_chars_count = get_line_text_excluding_margin(line).chars().count();
                    let start_char = if line_idx == min_line { min_col } else { 0 };
                    let end_char = if line_idx == max_line {
                        max_col
                    } else {
                        text_chars_count
                    };
                    *line = highlight_msg_line(line.clone(), start_char, end_char, highlight_style);
                }
            }
        }
        all_lines.extend(final_msg_lines);
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

pub(crate) fn dim_color(color: Color) -> Color {
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

pub(crate) fn dim_buffer(frame: &mut Frame, area: Rect) {
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

pub(crate) fn visual_width(s: &str) -> usize {
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

pub(crate) fn clean_and_truncate_to_visual_width(s: &str, max_w: usize) -> (String, usize) {
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

fn get_line_text_excluding_margin(line: &Line<'_>) -> String {
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

fn highlight_msg_line(
    line: Line<'static>,
    start_char: usize,
    end_char: usize,
    highlight_style: Style,
) -> Line<'static> {
    let mut new_spans = Vec::new();
    let mut current_offset = 0;

    let mut spans_iter = line.spans.into_iter();
    if let Some(first_span) = spans_iter.next() {
        if first_span.content == "    " {
            new_spans.push(first_span);
        } else {
            let content_chars: Vec<char> = first_span.content.chars().collect();
            let span_len = content_chars.len();
            let span_end = current_offset + span_len;
            if current_offset >= end_char || span_end <= start_char {
                new_spans.push(first_span);
            } else {
                let intersect_start = start_char.max(current_offset);
                let intersect_end = end_char.min(span_end);
                if intersect_start > current_offset {
                    let prefix_len = intersect_start - current_offset;
                    let prefix_text: String = content_chars[..prefix_len].iter().collect();
                    new_spans.push(Span::styled(prefix_text, first_span.style));
                }
                let middle_start_idx = intersect_start - current_offset;
                let middle_end_idx = intersect_end - current_offset;
                let middle_text: String = content_chars[middle_start_idx..middle_end_idx]
                    .iter()
                    .collect();
                new_spans.push(Span::styled(
                    middle_text,
                    first_span.style.patch(highlight_style),
                ));
                if span_end > intersect_end {
                    let suffix_start_idx = intersect_end - current_offset;
                    let suffix_text: String = content_chars[suffix_start_idx..].iter().collect();
                    new_spans.push(Span::styled(suffix_text, first_span.style));
                }
            }
            current_offset = span_end;
        }
    }

    for span in spans_iter {
        let content_chars: Vec<char> = span.content.chars().collect();
        let span_len = content_chars.len();
        let span_end = current_offset + span_len;

        if current_offset >= end_char || span_end <= start_char {
            new_spans.push(span);
        } else {
            let intersect_start = start_char.max(current_offset);
            let intersect_end = end_char.min(span_end);

            if intersect_start > current_offset {
                let prefix_len = intersect_start - current_offset;
                let prefix_text: String = content_chars[..prefix_len].iter().collect();
                new_spans.push(Span::styled(prefix_text, span.style));
            }

            let middle_start_idx = intersect_start - current_offset;
            let middle_end_idx = intersect_end - current_offset;
            let middle_text: String = content_chars[middle_start_idx..middle_end_idx]
                .iter()
                .collect();
            new_spans.push(Span::styled(middle_text, span.style.patch(highlight_style)));

            if span_end > intersect_end {
                let suffix_start_idx = intersect_end - current_offset;
                let suffix_text: String = content_chars[suffix_start_idx..].iter().collect();
                new_spans.push(Span::styled(suffix_text, span.style));
            }
        }
        current_offset = span_end;
    }

    Line::from(new_spans)
}
