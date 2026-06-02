use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap};

use crate::app::{App, Screen, SetupField, PendingTask};
use crate::tui::syntax::{parse_markdown_lines, wrap_lines, wrap_text_to_lines};

#[cfg(target_os = "windows")]
mod icons {
    pub const PROVIDER: &str = "";
    pub const SECURITY: &str = "";
    pub const TIP: &str = "TIP: ";
    pub const SAVE: &str = "";
    pub const CHAT_MODE: &str = " CHAT ";
    pub const SETTINGS_MODE: &str = " SETTINGS ";
    pub const MODELS_MODE: &str = " MODELS ";
    pub const SECURITY_MODE: &str = " SECURITY ";
    pub const SESSIONS_MODE: &str = " SESSIONS ";
    pub const CPU: &str = "";
    pub const IDLE: &str = "OK";
    pub const CHECK_ENABLED: &str = "Enabled";
    pub const CROSS_DISABLED: &str = "Disabled";
    pub const CHECK_SHOW_FULL: &str = "Show Full";
    pub const CROSS_LABEL_ONLY: &str = "Label Only";
    pub const SHELL_OK: &str = "+";
    pub const SHELL_ERR: &str = "-";
    pub const ACTIVE_MARKER: &str = " > ";
    pub const INACTIVE_MARKER: &str = "   ";
}

#[cfg(not(target_os = "windows"))]
mod icons {
    pub const PROVIDER: &str = "  ";
    pub const SECURITY: &str = "  ";
    pub const TIP: &str = "  TIP: ";
    pub const SAVE: &str = "  ";
    pub const CHAT_MODE: &str = " 󰍡 CHAT ";
    pub const SETTINGS_MODE: &str = "  SETTINGS ";
    pub const MODELS_MODE: &str = "  MODELS ";
    pub const SECURITY_MODE: &str = "  SECURITY ";
    pub const SESSIONS_MODE: &str = "  SESSIONS ";
    pub const CPU: &str = " ";
    pub const IDLE: &str = "";
    pub const CHECK_ENABLED: &str = "✔ Enabled";
    pub const CROSS_DISABLED: &str = "✗ Disabled";
    pub const CHECK_SHOW_FULL: &str = "✔ Show Full";
    pub const CROSS_LABEL_ONLY: &str = "✗ Label Only";
    pub const SHELL_OK: &str = "✓";
    pub const SHELL_ERR: &str = "✗";
    pub const ACTIVE_MARKER: &str = "  ";
    pub const INACTIVE_MARKER: &str = "   ";
}

fn get_theme(app: &App) -> crate::config::Theme {
    let raw_theme = if app.screen == Screen::Setup {
        app.setup.theme
    } else {
        app.chat.config.theme
    };
    crate::config::resolve_theme(raw_theme)
}

pub(crate) fn render(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Setup => render_setup(frame, app),
        Screen::Chat => render_chat(frame, app),
        Screen::Models => render_models(frame, app),
        Screen::Permissions => render_permissions(frame, app),
        Screen::Sessions => render_sessions(frame, app),
    }
}

fn render_setup(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let logo = logo_lines_for_area(area.width, 5);
    let logo_height = logo.len() as u16 + 1;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(logo_height),
            Constraint::Min(12),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(Paragraph::new(logo).alignment(Alignment::Center), chunks[0]);

    let api_key = if app.setup.api_key.is_empty() {
        "not set".to_owned()
    } else {
        let count = app.setup.api_key.chars().count();
        format!("{} ({} chars)", "*".repeat(count.min(12)), count)
    };

    // Split chunks[1] into fields area, tip area, and save button area
    let has_tip = app.setup.api_key.starts_with("sk-");
    let settings_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Min(8), // Left & Right Column panels
            Constraint::Length(if has_tip { 2 } else { 0 }), // Tip area
            Constraint::Length(3), // Save button area
        ])
        .split(chunks[1]);

    // Split settings_layout[0] horizontally into two columns
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(settings_layout[0]);

    // Determine active field groupings to dynamically light up panel borders!
    let left_active = matches!(
        app.setup.active_field,
        SetupField::ApiKey | SetupField::Model | SetupField::BaseUrl
    );
    let right_active = matches!(
        app.setup.active_field,
        SetupField::EnableCodebase
            | SetupField::EnableBash
            | SetupField::PermissionLevel
            | SetupField::ShowThoughts
            | SetupField::Theme
            | SetupField::RespectGitignore
    );

    // Left Column: Provider & Connection
    let left_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if left_active {
            Style::default().fg(Color::Rgb(236, 72, 153)) // Glow Pink/Magenta
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title(Span::styled(
            format!(" {}Provider & Connection ", icons::PROVIDER),
            Style::default().add_modifier(Modifier::BOLD),
        ))
        .padding(Padding::new(1, 1, 1, 1));

    let left_fields = vec![
        draw_setup_field(
            "API key",
            &api_key,
            app.setup.active_field == SetupField::ApiKey,
            Color::Rgb(236, 72, 153),
        ),
        Line::from(""),
        draw_setup_field(
            "Model",
            &app.setup.model,
            app.setup.active_field == SetupField::Model,
            Color::Rgb(168, 85, 247),
        ),
        Line::from(""),
        draw_setup_field(
            "Base URL",
            &app.setup.base_url,
            app.setup.active_field == SetupField::BaseUrl,
            Color::Rgb(59, 130, 246),
        ),
    ];

    frame.render_widget(Paragraph::new(left_fields).block(left_block), columns[0]);

    // Right Column: Security & Preferences
    let right_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if right_active {
            Style::default().fg(Color::Rgb(16, 185, 129)) // Glow Green/Emerald
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title(Span::styled(
            format!(" {}Security & Preferences ", icons::SECURITY),
            Style::default().add_modifier(Modifier::BOLD),
        ))
        .padding(Padding::new(1, 1, 1, 1));

    let right_fields = vec![
        draw_setup_field(
            "Codebase Tools",
            if app.setup.enable_codebase_tools { icons::CHECK_ENABLED } else { icons::CROSS_DISABLED },
            app.setup.active_field == SetupField::EnableCodebase,
            Color::Rgb(16, 185, 129),
        ),
        Line::from(""),
        draw_setup_field(
            "Bash Execution",
            if app.setup.enable_bash_tools { icons::CHECK_ENABLED } else { icons::CROSS_DISABLED },
            app.setup.active_field == SetupField::EnableBash,
            Color::Rgb(245, 158, 11),
        ),
        Line::from(""),
        draw_setup_field(
            "Security Mode",
            app.setup.permission_level.label(),
            app.setup.active_field == SetupField::PermissionLevel,
            Color::Rgb(239, 68, 68),
        ),
        Line::from(""),
        draw_setup_field(
            "Thoughts View",
            if app.setup.show_thoughts { icons::CHECK_SHOW_FULL } else { icons::CROSS_LABEL_ONLY },
            app.setup.active_field == SetupField::ShowThoughts,
            Color::Rgb(6, 182, 212),
        ),
        Line::from(""),
        draw_setup_field(
            "Theme",
            app.setup.theme.label(),
            app.setup.active_field == SetupField::Theme,
            Color::Rgb(251, 146, 60),
        ),
        Line::from(""),
        draw_setup_field(
            "Respect .gitignore",
            if app.setup.respect_gitignore { icons::CHECK_ENABLED } else { icons::CROSS_DISABLED },
            app.setup.active_field == SetupField::RespectGitignore,
            Color::Rgb(168, 85, 247), // Purple
        ),
    ];

    frame.render_widget(Paragraph::new(right_fields).block(right_block), columns[1]);

    // Render Tip Area if sk- key is active
    if has_tip {
        let tip_paragraph = Paragraph::new(Line::from(vec![
            Span::styled(icons::TIP, Style::default().fg(Color::Rgb(245, 158, 11)).add_modifier(Modifier::BOLD)),
            Span::styled("OpenAI key detected. Press ", Style::default()),
            Span::styled("Ctrl+A", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" to auto-apply OmniRoute defaults.", Style::default()),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(tip_paragraph, settings_layout[1]);
    }

    // Save Button
    let save_active = app.setup.active_field == SetupField::Save;
    let save_style = if save_active {
        Style::default().bg(Color::Rgb(59, 130, 246)).fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(59, 130, 246)).add_modifier(Modifier::BOLD)
    };
    
    let save_text = if save_active {
        format!(" {}SAVE AND START ASSISTANT ", icons::SAVE)
    } else {
        format!(" {}Save and Start Assistant ", icons::SAVE)
    };

    let save_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if save_active { Color::Rgb(59, 130, 246) } else { Color::DarkGray }))
        .padding(Padding::horizontal(1));

    let save_paragraph = Paragraph::new(Line::from(vec![
        Span::styled(save_text, save_style)
    ]))
    .alignment(Alignment::Center)
    .block(save_block);

    frame.render_widget(save_paragraph, settings_layout[2]);

    render_statusbar(frame, app, chunks[2]);
}

fn draw_setup_field(
    label: &str,
    value: &str,
    active: bool,
    color: Color,
) -> Line<'static> {
    let marker = if active { icons::ACTIVE_MARKER } else { icons::INACTIVE_MARKER };
    let marker_style = if active {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    // Active label is bold default terminal text. Inactive is DarkGray (remapped by terminal theme).
    let label_style = if active {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    // Active value uses the theme accent color. Inactive uses default terminal text.
    let value_style = if active {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::styled(marker, marker_style),
        Span::styled(format!("{:<18}", label), label_style),
        Span::styled(value.to_owned(), value_style),
    ])
}

fn render_messages(frame: &mut Frame, app: &App, area: Rect) {
    let shell_focused = app.chat.shell_focused;
    let block_border_style = if shell_focused {
        Style::default().fg(Color::Rgb(59, 130, 246)).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    let title = if shell_focused {
        " 󰍡 Conversations (ACTIVE FOCUS - Tab to return) "
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
        let lines = welcome_lines(app, welcome_area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), welcome_area);
        return;
    }

    let inner_area = block.inner(area);
    let mut all_lines = Vec::new();
    let width = inner_area.width.saturating_sub(4);

    let push_margin = |lines: &mut Vec<Line<'static>>| {
        if let Some(last) = lines.last()
            && (!last.spans.is_empty() || last.width() > 0) {
                lines.push(Line::from(""));
            }
    };

    let mut prev_is_tool = false;

    for message in &app.chat.messages {
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
                let border_color = if shell_focused {
                    Color::Rgb(59, 130, 246)
                } else {
                    Color::DarkGray
                };
                let border_style = Style::default().fg(border_color);
                
                let title_style = if shell_focused {
                    Style::default().fg(Color::Rgb(59, 130, 246)).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                };

                let icon = if message.shell_success { icons::SHELL_OK } else { icons::SHELL_ERR };
                let title = format!("{icon} Shell {}", message.shell_cmd);
                
                msg_lines.push(Line::from(Span::styled(format!("╭{}╮", "─".repeat(width as usize)), border_style)));
                
                let title_len = title.chars().count();
                let title_w = (width as usize).saturating_sub(1);
                let title_pad = title_w.saturating_sub(title_len);
                let padded_title = format!("{}{}", title, " ".repeat(title_pad));

                msg_lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(padded_title, title_style),
                    Span::styled("│", border_style),
                ]));
                
                for line in content.lines() {
                    let line_len = line.chars().count();
                    let body_w = (width as usize).saturating_sub(2);
                    let body_pad = body_w.saturating_sub(line_len);
                    let padded_line = format!("{}{}", line, " ".repeat(body_pad));

                    msg_lines.push(Line::from(vec![
                        Span::styled("│  ", border_style),
                        Span::styled(padded_line, Style::default().fg(Color::DarkGray)),
                        Span::styled("│", border_style),
                    ]));
                }
                msg_lines.push(Line::from(Span::styled(format!("╰{}╯", "─".repeat(width as usize)), border_style)));
            } else {
                let lines_count = content.lines().count();
                let limit = 500;
                let (display_content, is_truncated) = if lines_count > limit {
                    let truncated: String = content.lines().take(limit).collect::<Vec<&str>>().join("\n");
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
                            crate::config::Theme::Dark | crate::config::Theme::Auto => (Color::Rgb(255, 255, 255), Color::Rgb(0, 0, 0)),
                            crate::config::Theme::Light => (Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255)),
                        };
                        let user_style = Style::default().bg(user_bg).fg(user_fg);
                        let block_width = inner_area.width as usize;
                        
                        msg_lines.push(Line::from(Span::styled(" ".repeat(block_width), user_style)));
                        
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
                        
                        msg_lines.push(Line::from(Span::styled(" ".repeat(block_width), user_style)));
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
                *message.cached_wrapped.borrow_mut() = Some((width as usize, get_theme(app), msg_lines.clone()));
            }
            msg_lines
        };

        if !all_lines.is_empty() && (is_shell || message.author == "You" || message.author == "System" || !is_tool || !prev_is_tool) {
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
    frame.render_widget(
        Paragraph::new(visible_lines),
        inner_area,
    );
}

fn render_chat(frame: &mut Frame, app: &App) {
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
        ((area.width as u32 * 70 / 100) as u16).saturating_sub(4).max(1)
    } else {
        area.width.saturating_sub(4).max(1)
    };
    let wrapped_lines = wrap_text_to_lines(app.chat.input.as_str(), input_inner_w as usize);
    let display_lines = wrapped_lines.len() as u16;
    let input_height = display_lines.clamp(1, 5) + 2;

    let (messages_area, suggestions_area, queue_area, input_area, statusbar_area, logo_area, tips_area) = if app.chat.messages.is_empty() {
        let empty_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(25),      // Top space (centers the logo vertically)
                Constraint::Length(6),           // Logo (5 lines + 1 spacer)
                Constraint::Length(input_height), // Input box
                Constraint::Length(2),           // Spacer
                Constraint::Length(1),           // Tips line
                Constraint::Min(0),              // Bottom space
                Constraint::Length(1),           // Status bar
            ])
            .split(area);
            
        // Sub-split empty_chunks[2] horizontally to make the input box beautifully centered (e.g. 70% width)
        let centered_input_box = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(15),
                Constraint::Percentage(70),
                Constraint::Percentage(15),
            ])
            .split(empty_chunks[2])[1];

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
            if suggestion_height > 0 { Some(normal_chunks[1]) } else { None },
            if queue_height > 0 { Some(normal_chunks[2]) } else { None },
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
            Span::styled("Ctrl+S", Style::default().fg(Color::Rgb(236, 72, 153)).add_modifier(Modifier::BOLD)),
            Span::styled(" Setup  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+P", Style::default().fg(Color::Rgb(168, 85, 247)).add_modifier(Modifier::BOLD)),
            Span::styled(" Model  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("/help", Style::default().fg(Color::Rgb(59, 130, 246)).add_modifier(Modifier::BOLD)),
            Span::styled(" Help", Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(tips_line).alignment(Alignment::Center), tips_area);
    }

    if let Some(messages_area) = messages_area {
        render_messages(frame, app, messages_area);
    }

    if let Some(suggestions_area) = suggestions_area {
        render_command_suggestions(frame, app, suggestions_area);
    }

    if let Some(queue_area) = queue_area {
        let mut queue_lines = vec![
            Line::from(Span::styled(" Queued Prompts: ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)))
        ];
        for (idx, item) in app.chat.message_queue.iter().enumerate().take(3) {
            let truncated_item = if item.chars().count() > 60 {
                let s: String = item.chars().take(57).collect();
                format!("{}...", s)
            } else {
                item.clone()
            };
            queue_lines.push(Line::from(vec![
                Span::styled(format!("  [{}] ", idx + 1), Style::default().fg(Color::DarkGray)),
                Span::styled(truncated_item, Style::default().fg(Color::DarkGray)),
            ]));
        }
        if app.chat.message_queue.len() > 3 {
            let remaining = app.chat.message_queue.len() - 3;
            queue_lines.push(Line::from(Span::styled(format!("  ... and {} more", remaining), Style::default().fg(Color::DarkGray))));
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
        frame.set_cursor_position((
            input_box.x + 2 + cursor_x,
            target_y,
        ));
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
            let old_str = args.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
            let new_str = args.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
            
            text.push(Line::from(vec![
                Span::styled("File: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(path, Style::default().fg(Color::Cyan)),
            ]));
            text.push(Line::from(""));
            text.push(Line::from(Span::styled("Diff Preview:", Style::default().add_modifier(Modifier::BOLD))));
            
            for line in old_str.lines() {
                let formatted = format!("- {line}");
                let padded = format!("{formatted:<width$}", width = inner_width);
                text.push(Line::from(Span::styled(
                    padded,
                    Style::default().fg(Color::Rgb(255, 180, 180)).bg(Color::Rgb(70, 20, 20))
                )));
            }
            for line in new_str.lines() {
                let formatted = format!("+ {line}");
                let padded = format!("{formatted:<width$}", width = inner_width);
                text.push(Line::from(Span::styled(
                    padded,
                    Style::default().fg(Color::Rgb(180, 255, 180)).bg(Color::Rgb(20, 60, 20))
                )));
            }
            text.push(Line::from(""));
        } else if name == "edit_files" {
            if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                for (idx, edit_val) in edits.iter().enumerate() {
                    let path = edit_val.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let old_str = edit_val.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
                    let new_str = edit_val.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
                    
                    text.push(Line::from(vec![
                        Span::styled(format!("Edit #{} (", idx + 1), Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(path, Style::default().fg(Color::Cyan)),
                        Span::styled("):", Style::default().add_modifier(Modifier::BOLD)),
                    ]));
                    
                    for line in old_str.lines() {
                        let formatted = format!("- {line}");
                        let padded = format!("{formatted:<width$}", width = inner_width);
                        text.push(Line::from(Span::styled(
                            padded,
                            Style::default().fg(Color::Rgb(255, 180, 180)).bg(Color::Rgb(70, 20, 20))
                        )));
                    }
                    for line in new_str.lines() {
                        let formatted = format!("+ {line}");
                        let padded = format!("{formatted:<width$}", width = inner_width);
                        text.push(Line::from(Span::styled(
                            padded,
                            Style::default().fg(Color::Rgb(180, 255, 180)).bg(Color::Rgb(20, 60, 20))
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
            text.push(Line::from(Span::styled("Content to Write:", Style::default().add_modifier(Modifier::BOLD))));
            
            for line in content.lines() {
                let formatted = format!("+ {line}");
                let padded = format!("{formatted:<width$}", width = inner_width);
                text.push(Line::from(Span::styled(
                    padded,
                    Style::default().fg(Color::Rgb(180, 255, 180)).bg(Color::Rgb(20, 60, 20))
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
            Span::styled("[Y]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" Allow   "),
            Span::styled("[N]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
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

fn render_permissions(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .split(area);

    render_messages(frame, app, chunks[0]);

    let options = crate::app::PermissionPickerState::options();
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (label, desc, _))| {
            if i == app.permissions.selected {
                ListItem::new(Line::from(vec![
                    Span::styled("> ", Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow)),
                    Span::styled(format!("{label}: "), Style::default().add_modifier(Modifier::BOLD).fg(Color::White)),
                    Span::styled(*desc, Style::default().fg(Color::White)),
                ])).style(Style::default().bg(Color::Rgb(60, 60, 75)))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{label}: "), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(*desc),
                ]))
            }
        })
        .collect();

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Select Permission Level ")
                .padding(Padding::horizontal(1)),
        ),
        chunks[1],
    );

    render_statusbar(frame, app, chunks[2]);
}

fn render_models(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(area);

    let items = app
        .models
        .models
        .iter()
        .enumerate()
        .map(|(index, model): (usize, &String)| {
            let model = model.trim_start_matches("models/");
            if index == app.models.selected {
                ListItem::new(Line::from(vec![
                    Span::styled("> ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        model.to_owned(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]))
            } else {
                ListItem::new(format!("  {model}"))
            }
        })
        .collect::<Vec<_>>();

    let mut state = ListState::default().with_selected(Some(app.models.selected));
    keep_selected_visible(
        &mut state,
        app.models.selected,
        chunks[0].height.saturating_sub(2),
    );

    frame.render_stateful_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Models ")
                .padding(Padding::horizontal(1)),
        ),
        chunks[0],
        &mut state,
    );

    render_statusbar(frame, app, chunks[1]);
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

fn render_statusbar(frame: &mut Frame, app: &App, area: Rect) {
    let (mode_text, mode_bg, mode_fg) = match app.screen {
        Screen::Chat => (icons::CHAT_MODE, Color::Rgb(59, 130, 246), Color::Black), // Vibrant blue
        Screen::Setup => (icons::SETTINGS_MODE, Color::Rgb(236, 72, 153), Color::Black), // Magenta/Pink
        Screen::Models => (icons::MODELS_MODE, Color::Rgb(168, 85, 247), Color::Black), // Purple
        Screen::Permissions => (icons::SECURITY_MODE, Color::Rgb(245, 158, 11), Color::Black), // Amber/Yellow
        Screen::Sessions => (icons::SESSIONS_MODE, Color::Rgb(16, 185, 129), Color::Black), // Emerald green
    };

    let mode_len = mode_text.chars().count() as u16;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(mode_len), // Dynamic size perfectly fitting the capsule!
            Constraint::Min(20),
            Constraint::Length(32),
        ])
        .split(area);

    let theme = get_theme(app);
    let (bar_bg, bar_fg) = match theme {
        crate::config::Theme::Dark | crate::config::Theme::Auto => (Color::Rgb(255, 255, 255), Color::Rgb(0, 0, 0)),
        crate::config::Theme::Light => (Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255)),
    };
    let base_style = Style::default().bg(bar_bg).fg(bar_fg);

    // 1. Render left mode block (powerline capsule style)
    let mode_paragraph = Paragraph::new(mode_text)
        .style(Style::default().bg(mode_bg).fg(mode_fg).add_modifier(Modifier::BOLD));
    frame.render_widget(mode_paragraph, chunks[0]);

    // 2. Build middle status segment (ultra-minimal, animated)
    let is_busy = app.busy_label().is_some();
    let status_str = app.busy_label().unwrap_or_else(|| app.status.clone());
    let status_icon = if is_busy {
        #[cfg(target_os = "windows")]
        let spinner_frames = ["-", "\\", "|", "/"];
        #[cfg(not(target_os = "windows"))]
        let spinner_frames = ["◐", "◓", "◑", "◒"];
        spinner_frames[(app.tick / 2) % spinner_frames.len()]
    } else {
        icons::IDLE
    };
    let status_color = if is_busy { Color::Rgb(245, 158, 11) } else { Color::Rgb(34, 197, 94) }; // Amber vs Green

    let middle_spans = vec![
        Span::styled(format!(" {status_icon} "), Style::default().fg(status_color)),
        Span::styled(format!("{} ", status_str), Style::default().add_modifier(Modifier::BOLD)),
    ];

    let middle_paragraph = Paragraph::new(Line::from(middle_spans)).style(base_style);
    frame.render_widget(middle_paragraph, chunks[1]);

    // 3. Build right model segment (right-aligned)
    let model_name = match app.screen {
        Screen::Setup => &app.setup.model,
        _ => &app.chat.config.model,
    };
    let right_text = format!(" {}{} ", icons::CPU, model_name);
    let right_paragraph = Paragraph::new(right_text)
        .alignment(Alignment::Right)
        .style(base_style);
    frame.render_widget(right_paragraph, chunks[2]);
}

fn logo_lines() -> Vec<Line<'static>> {
    let lines = vec![
        "   ▄▄                                              ▄▄       ",
        "   ██                     ▀▀                       ██       ",
        "▄████  ▀▀█▄ ████▄ ██   ██ ██  ████▄ ▄████ ▄███▄ ▄████ ▄█▀█▄ ",
        "██ ██ ▄█▀██ ██ ▀▀ ██ █ ██ ██  ██ ██ ██    ██ ██ ██ ██ ██▄█▀ ",
        "▀████ ▀█▄██ ██     ██▀██  ██▄ ██ ██ ▀████ ▀███▀ ▀████ ▀█▄▄▄ ",
    ];

    lines
        .into_iter()
        .map(|line| {
            Line::from(Span::styled(
                line,
                Style::default(),
            ))
        })
        .collect()
}

fn logo_lines_for_area(width: u16, max_height: u16) -> Vec<Line<'static>> {
    let lines = logo_lines();
    if logo_fits(&lines, width, max_height) {
        lines
    } else {
        vec![Line::from(Span::styled(
            "darwincode",
            Style::default(),
        ))]
    }
}

fn logo_fits(lines: &[Line<'_>], width: u16, max_height: u16) -> bool {
    !lines.is_empty()
        && lines.len() <= max_height as usize
        && lines.iter().all(|line| line.width() <= width as usize)
}

fn keep_selected_visible(state: &mut ListState, selected: usize, viewport_height: u16) {
    let visible_rows = viewport_height.max(1) as usize;
    let offset = if selected >= visible_rows {
        selected + 1 - visible_rows
    } else {
        0
    };

    *state.offset_mut() = offset;
}

fn render_sessions(frame: &mut Frame, app: &App) {
    let theme = get_theme(app);
    let (label_color, query_color) = match theme {
        crate::config::Theme::Light => (Color::Blue, Color::Black),
        _ => (Color::Cyan, Color::White),
    };

    let area = frame.area();
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(area);

    let session_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(main_chunks[0]);

    let filter_text = format!(" {}", app.sessions.query);
    let filter_para = Paragraph::new(Line::from(vec![
        Span::styled(" Search: ", Style::default().fg(label_color).add_modifier(Modifier::BOLD)),
        Span::styled(filter_text, Style::default().fg(query_color)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Filter Sessions ")
    );
    frame.render_widget(filter_para, session_chunks[0]);

    let filtered = app.sessions.filtered_sessions();
    let (sel_bg, sel_fg_id, sel_fg_snippet) = match theme {
        crate::config::Theme::Light => (Color::Rgb(220, 220, 230), Color::Black, Color::Black),
        _ => (Color::Rgb(40, 40, 50), Color::White, Color::White),
    };
    let (unsel_fg_id, unsel_fg_snippet) = match theme {
        crate::config::Theme::Light => (Color::Rgb(50, 50, 50), Color::Rgb(100, 100, 100)),
        _ => (Color::Gray, Color::DarkGray),
    };

    let items = filtered
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let id = &session.id;
            let snippet = &session.snippet;
            
            if index == app.sessions.selected {
                ListItem::new(Line::from(vec![
                    Span::styled("> ", Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow)),
                    Span::styled(format!("{:<30} ", id), Style::default().add_modifier(Modifier::BOLD).fg(sel_fg_id)),
                    Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(snippet.clone(), Style::default().fg(sel_fg_snippet)),
                ])).style(Style::default().bg(sel_bg))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<30} ", id), Style::default().fg(unsel_fg_id)),
                    Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(snippet.clone(), Style::default().fg(unsel_fg_snippet)),
                ]))
            }
        })
        .collect::<Vec<_>>();

    let selected_opt = if filtered.is_empty() { None } else { Some(app.sessions.selected) };
    let mut state = ListState::default().with_selected(selected_opt);
    if !filtered.is_empty() {
        keep_selected_visible(
            &mut state,
            app.sessions.selected,
            session_chunks[1].height.saturating_sub(2),
        );
    }

    frame.render_stateful_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Resume Chat Session ")
                .padding(Padding::horizontal(1)),
        ),
        session_chunks[1],
        &mut state,
    );

    render_statusbar(frame, app, main_chunks[1]);
}

fn welcome_lines(_app: &App, area: Rect) -> Vec<Line<'static>> {
    let max_logo_height = area.height.saturating_sub(2).max(1);
    logo_lines_for_area(area.width, max_logo_height)
}
