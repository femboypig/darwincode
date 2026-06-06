use ratatui::Frame;
use ratatui::layout::{Alignment, Rect, Layout, Direction, Constraint, Padding};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap, Scrollbar, ScrollbarOrientation, ScrollbarState};
use crate::app::App;
use crate::app::chat::{TodoItem, TodoPriority, TodoStatus};
use crate::tui::render::chat::message_list::centered_rect;
use crate::tui::syntax::wrap_text_to_lines;

pub(crate) fn render_confirm_modal(frame: &mut Frame, app: &App, area: Rect) {
    let Some(crate::app::PendingTask::ConfirmFunction { name, args }) = &app.proc.pending else {
        return;
    };
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme.background_panel.unwrap_or({
        if active_theme.is_light {
            Color::Rgb(240, 240, 240)
        } else {
            Color::Rgb(24, 24, 24)
        }
    });
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
        let current_scroll = app.ui.confirm_scroll.get().min(max_scroll as u16);
        app.ui.confirm_scroll.set(current_scroll);

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

pub(crate) fn render_ask_user_in_input_box(frame: &mut Frame, app: &App, area: Rect) {
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

    let q_wrapped = wrap_text_to_lines(app.ui.ask_user.question.as_str(), inner.width as usize);
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
    let footer_y = inner.bottom().saturating_sub(2);
    let total_options = app.ui.ask_user.options.len() + 1;
    let selected = app.ui.ask_user.selected_idx;

    let mut row_y = list_start_y;
    for idx in 0..total_options {
        if row_y >= footer_y.saturating_sub(1) {
            break;
        }
        let is_selected = idx == selected;
        let is_custom = idx == app.ui.ask_user.options.len();
        let label: &str = if is_custom {
            "Type your own answer"
        } else {
            &app.ui.ask_user.options[idx]
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

        if is_custom && app.ui.ask_user.is_custom && row_y < footer_y.saturating_sub(1) {
            let prefix_len = format!("{}.  ", idx + 1).chars().count();
            let indent = " ".repeat(prefix_len);
            let display = if app.ui.ask_user.custom_input.is_empty() {
                format!("{}Type your answer...", indent)
            } else {
                format!("{}{}", indent, app.ui.ask_user.custom_input)
            };
            let input_style = if app.ui.ask_user.custom_input.is_empty() {
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
                + if app.ui.ask_user.custom_input.is_empty() {
                    0
                } else {
                    app.ui.ask_user.custom_input.chars().count() as u16
                };
            if cx < inner.right() {
                frame.set_cursor_position((cx, row_y));
            }
            row_y += 1;
        }
    }

    if footer_y < inner.bottom() {
        let footer = if app.ui.ask_user.is_custom {
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

pub(crate) fn render_permissions_in_input_box(frame: &mut Frame, app: &App, area: Rect) {
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
    let selected = app.ui.permissions.selected.min(total.saturating_sub(1));

    let footer_y = inner.bottom().saturating_sub(2);

    let mut row_y = inner.y + 2;
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

pub(crate) fn render_model_picker_modal(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme.background_panel.unwrap_or({
        if active_theme.is_light {
            Color::Rgb(240, 240, 240)
        } else {
            Color::Rgb(24, 24, 24)
        }
    });
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

    let search_line = if app.ui.models.query.is_empty() {
        Line::from(Span::styled("Search", Style::default().fg(placeholder_fg)))
    } else {
        Line::from(Span::styled(
            app.ui.models.query.clone(),
            Style::default().fg(modal_fg),
        ))
    };
    frame.render_widget(Paragraph::new(search_line), search_content);

    let cursor_x = search_content.x + app.ui.models.query.chars().count() as u16;
    if cursor_x < search_content.right() {
        frame.set_cursor_position((cursor_x, search_content.y));
    }

    let list_area = chunks[4];
    let filtered = app.ui.models.filtered_indices();
    let active_model = app.chat.config.model.as_str();

    if filtered.is_empty() {
        let msg = if app.ui.models.query.is_empty() {
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
    let selected = app.ui.models.selected.min(total.saturating_sub(1));
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
        let model = &app.ui.models.models[filtered[idx]];
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

pub(crate) fn render_theme_picker_modal(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme.background_panel.unwrap_or({
        if active_theme.is_light {
            Color::Rgb(240, 240, 240)
        } else {
            Color::Rgb(24, 24, 24)
        }
    });
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

    let search_line = if app.ui.theme_picker.query.is_empty() {
        Line::from(Span::styled("Search", Style::default().fg(placeholder_fg)))
    } else {
        Line::from(Span::styled(
            app.ui.theme_picker.query.clone(),
            Style::default().fg(modal_fg),
        ))
    };
    frame.render_widget(Paragraph::new(search_line), search_content);

    let cursor_x = search_content.x + app.ui.theme_picker.query.chars().count() as u16;
    if cursor_x < search_content.right() {
        frame.set_cursor_position((cursor_x, search_content.y));
    }

    let list_area = chunks[4];
    let filtered = app.ui.theme_picker.filtered_indices();

    if filtered.is_empty() {
        let msg = if app.ui.theme_picker.query.is_empty() {
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
    let selected = app.ui.theme_picker.selected.min(total.saturating_sub(1));
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
        let theme = &app.ui.theme_picker.themes[filtered[idx]];
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

pub(crate) fn render_agent_picker_modal(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme.background_panel.unwrap_or({
        if active_theme.is_light {
            Color::Rgb(240, 240, 240)
        } else {
            Color::Rgb(24, 24, 24)
        }
    });
    let modal_fg = active_theme.text;
    let placeholder_fg = active_theme.text_muted;
    let list_fg = active_theme.text;
    let hint_fg = active_theme.text_muted;
    let active_bullet_color = active_theme.success;
    let active_bullet_color_on_select = active_theme.secondary;
    let select_bg = active_theme.accent;

    let popup_area = centered_rect(42, 48, area);
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
            "Select agent",
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

    let search_line = if app.ui.agent_picker.query.is_empty() {
        Line::from(Span::styled("Search", Style::default().fg(placeholder_fg)))
    } else {
        Line::from(Span::styled(
            app.ui.agent_picker.query.clone(),
            Style::default().fg(modal_fg),
        ))
    };
    frame.render_widget(Paragraph::new(search_line), search_content);

    let cursor_x = search_content.x + app.ui.agent_picker.query.chars().count() as u16;
    if cursor_x < search_content.right() {
        frame.set_cursor_position((cursor_x, search_content.y));
    }

    let list_area = chunks[4];
    let filtered = app.ui.agent_picker.filtered_indices();

    if filtered.is_empty() {
        let msg = if app.ui.agent_picker.query.is_empty() {
            "No agents available"
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
    let selected = app.ui.agent_picker.selected.min(total.saturating_sub(1));
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
        let (agent_id, display_name) = &app.ui.agent_picker.agents[filtered[idx]];
        let is_selected = idx == selected;
        let is_active = agent_id == &app.core.active_agent;

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
            Paragraph::new(Span::styled(display_name, text_style)),
            row_cols[3],
        );
    }
}

pub(crate) fn render_todos(frame: &mut Frame, app: &App, area: Rect) {
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

    let render_priority = |p: &TodoPriority| -> Span<'static> {
        match p {
            TodoPriority::High => Span::styled("!! ", Style::default().fg(Color::Rgb(239, 68, 68))),
            TodoPriority::Medium => Span::styled("!  ", Style::default().fg(Color::Rgb(245, 158, 11))),
            TodoPriority::Low => Span::styled("   ", Style::default()),
        }
    };

    let in_progress: Vec<&TodoItem> = app
        .chat
        .todos
        .iter()
        .filter(|t| t.status == TodoStatus::InProgress)
        .collect();
    let pending: Vec<&TodoItem> = app
        .chat
        .todos
        .iter()
        .filter(|t| t.status == TodoStatus::Pending)
        .collect();
    let completed: Vec<&TodoItem> = app
        .chat
        .todos
        .iter()
        .filter(|t| t.status == TodoStatus::Completed)
        .collect();
    let cancelled: Vec<&TodoItem> = app
        .chat
        .todos
        .iter()
        .filter(|t| t.status == TodoStatus::Cancelled)
        .collect();

    let mut all_sorted_todos = Vec::new();
    all_sorted_todos.extend(in_progress);
    all_sorted_todos.extend(pending);
    all_sorted_todos.extend(completed);
    all_sorted_todos.extend(cancelled);

    let tasks_area = todo_chunks[2];

    for item in all_sorted_todos {
        let (status_bullet, item_style) = match item.status {
            TodoStatus::InProgress => (
                Span::styled("● ", Style::default().fg(Color::Rgb(234, 179, 8))),
                Style::default().fg(Color::Rgb(220, 220, 220)),
            ),
            TodoStatus::Completed => (
                Span::styled("● ", Style::default().fg(Color::Rgb(34, 197, 94))),
                Style::default().fg(Color::DarkGray),
            ),
            TodoStatus::Cancelled => (
                Span::styled("◌ ", Style::default().fg(Color::DarkGray)),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
            TodoStatus::Pending => (
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
