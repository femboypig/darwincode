use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::{App, SetupField};
use crate::tui::render::icons::icons;

pub(crate) fn render_setup_modal(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme
        .background_panel
        .unwrap_or(Color::Rgb(24, 24, 24));
    let dim_text = active_theme.text_muted;

    // Centered modal popup: 55% width, 65% height
    let popup_area = crate::tui::render::chat::centered_rect(55, 65, area);
    frame.render_widget(ratatui::widgets::Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(modal_bg)),
        popup_area,
    );
    app.ui.setup.modal_area.set(Some(popup_area));

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

    let api_key_display = if app.ui.setup.api_key.is_empty() {
        "not set".to_owned()
    } else {
        let count = app.ui.setup.api_key.chars().count();
        format!("{} ({} chars)", "*".repeat(count.min(12)), count)
    };

    let has_tip = app.ui.setup.api_key.starts_with("sk-");
    let tip_height = if has_tip { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),          // Title
            Constraint::Length(1),          // Separator
            Constraint::Min(1),             // Fields
            Constraint::Length(1),          // Separator
            Constraint::Length(1),          // Footer tips
            Constraint::Length(tip_height), // OpenAI tip
        ])
        .split(content);

    // Title row (indented by 1)
    let title_area = Rect {
        x: chunks[0].x + 1,
        y: chunks[0].y,
        width: chunks[0].width.saturating_sub(2),
        height: 1,
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{} DarwinCode Assistant Settings", icons::SETTINGS_MODE),
            Style::default()
                .fg(active_theme.primary)
                .add_modifier(Modifier::BOLD),
        ))),
        title_area,
    );

    // Header separator
    frame.render_widget(
        Paragraph::new("─".repeat(content.width as usize)).style(Style::default().fg(dim_text)),
        chunks[1],
    );

    // Body (Fields)
    let body_area = chunks[2];

    let fields: Vec<(&str, String, SetupField, Color)> = vec![
        (
            "API Key",
            api_key_display,
            SetupField::ApiKey,
            Color::Rgb(236, 72, 153),
        ),
        (
            "Model",
            app.ui.setup.model.clone(),
            SetupField::Model,
            Color::Rgb(168, 85, 247),
        ),
        (
            "Base URL",
            app.ui.setup.base_url.clone(),
            SetupField::BaseUrl,
            Color::Rgb(59, 130, 246),
        ),
        (
            "Codebase Tools",
            (if app.ui.setup.enable_codebase_tools {
                icons::CHECK_ENABLED
            } else {
                icons::CROSS_DISABLED
            })
            .to_owned(),
            SetupField::EnableCodebase,
            Color::Rgb(16, 185, 129),
        ),
        (
            "Bash Execution",
            (if app.ui.setup.enable_bash_tools {
                icons::CHECK_ENABLED
            } else {
                icons::CROSS_DISABLED
            })
            .to_owned(),
            SetupField::EnableBash,
            Color::Rgb(245, 158, 11),
        ),
        (
            "Security Mode",
            app.ui.setup.permission_level.label().to_owned(),
            SetupField::PermissionLevel,
            Color::Rgb(239, 68, 68),
        ),
        (
            "Thoughts View",
            (if app.ui.setup.show_thoughts {
                icons::CHECK_SHOW_FULL
            } else {
                icons::CROSS_LABEL_ONLY
            })
            .to_owned(),
            SetupField::ShowThoughts,
            Color::Rgb(6, 182, 212),
        ),
        (
            "Theme",
            app.ui.setup.theme.label().to_owned(),
            SetupField::Theme,
            Color::Rgb(251, 146, 60),
        ),
        (
            "Respect Ignore Rules",
            (if app.ui.setup.respect_ignore_rules {
                icons::CHECK_ENABLED
            } else {
                icons::CROSS_DISABLED
            })
            .to_owned(),
            SetupField::RespectIgnoreRules,
            Color::Rgb(168, 85, 247),
        ),
        (
            "Trust Workspace",
            (if app.ui.setup.trust_workspace {
                icons::CHECK_ENABLED
            } else {
                icons::CROSS_DISABLED
            })
            .to_owned(),
            SetupField::TrustWorkspace,
            Color::Rgb(244, 63, 94),
        ),
    ];

    let total_lines = fields.len() + 1; // fields + Save button
    let viewport = body_area.height as usize;
    let active_idx = app.ui.setup.active_field.index();
    let start = if total_lines <= viewport || active_idx < viewport / 2 {
        0
    } else if active_idx >= total_lines - viewport / 2 {
        total_lines - viewport
    } else {
        active_idx - viewport / 2
    };
    let visible_count = viewport.min(total_lines.saturating_sub(start));

    for offset in 0..visible_count {
        let idx = start + offset;
        let row_y = body_area.y + offset as u16;
        if row_y >= body_area.bottom() {
            break;
        }
        let row_area = Rect {
            x: body_area.x,
            y: row_y,
            width: body_area.width,
            height: 1,
        };

        let is_save = idx == fields.len();
        let (label, value, is_active) = if is_save {
            (
                "Save and Start Assistant".to_owned(),
                String::new(),
                app.ui.setup.active_field == SetupField::Save,
            )
        } else {
            let (label, value, _, _) = &fields[idx];
            (
                label.to_string(),
                value.clone(),
                app.ui.setup.active_field == fields[idx].2,
            )
        };

        let row_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Length(22),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(row_area);

        let marker = if is_active {
            icons::ACTIVE_MARKER
        } else {
            icons::INACTIVE_MARKER
        };
        let marker_style = if is_active {
            Style::default()
                .fg(active_theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(dim_text)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(marker, marker_style)),
            row_cols[0],
        );

        let label_style = if is_active {
            Style::default()
                .fg(active_theme.text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(dim_text)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(label, label_style)),
            row_cols[2],
        );

        frame.render_widget(
            Paragraph::new(Span::styled(" ", Style::default())),
            row_cols[3],
        );

        if is_save {
            let save_style = if is_active {
                Style::default()
                    .bg(active_theme.primary)
                    .fg(active_theme.background_panel.unwrap_or(Color::Black))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(active_theme.primary)
                    .add_modifier(Modifier::BOLD)
            };
            let save_text = if is_active {
                format!(" {}SAVE AND START ASSISTANT ", icons::SAVE)
            } else {
                format!(" {}Save and Start Assistant ", icons::SAVE)
            };
            frame.render_widget(
                Paragraph::new(Span::styled(save_text, save_style)),
                row_cols[4],
            );
        } else {
            let field_color = fields[idx].3;
            let is_editing = is_active && app.ui.setup.is_editing;
            let value_style = if is_active {
                if is_editing {
                    Style::default()
                        .fg(active_theme.success)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(field_color)
                        .add_modifier(Modifier::BOLD)
                }
            } else {
                Style::default().fg(active_theme.text)
            };
            frame.render_widget(
                Paragraph::new(Span::styled(value.clone(), value_style)),
                row_cols[4],
            );

            if is_editing {
                let cursor_x = row_cols[4].x + value.chars().count() as u16;
                if cursor_x < row_cols[4].right() {
                    frame.set_cursor_position((cursor_x, row_y));
                }
            }
        }
    }

    // Footer separator
    frame.render_widget(
        Paragraph::new("─".repeat(content.width as usize)).style(Style::default().fg(dim_text)),
        chunks[3],
    );

    // Footer
    let footer_text = if app.ui.setup.is_editing {
        " Up/Down: Move • Enter: Save field • Esc: Cancel editing "
    } else {
        " Up/Down: Move • Enter/Space: Edit/Toggle • Left/Right: Cycle options • Esc: Exit "
    };
    frame.render_widget(
        Paragraph::new(Span::styled(footer_text, Style::default().fg(dim_text)))
            .alignment(Alignment::Center),
        chunks[4],
    );

    if has_tip {
        let tip_paragraph = Paragraph::new(Line::from(vec![
            Span::styled(
                icons::TIP,
                Style::default()
                    .fg(Color::Rgb(245, 158, 11))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "OpenAI key detected. Press ",
                Style::default().fg(active_theme.text),
            ),
            Span::styled(
                "Ctrl+A",
                Style::default()
                    .fg(active_theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to auto-apply OmniRoute defaults.",
                Style::default().fg(active_theme.text),
            ),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(tip_paragraph, chunks[5]);
    }
}
