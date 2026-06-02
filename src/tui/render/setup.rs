use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};

use crate::app::{App, SetupField};
use crate::tui::render::icons::icons;
use crate::tui::render::logo::logo_lines_for_area;
use crate::tui::render::render_statusbar;

pub(crate) fn render_setup(frame: &mut Frame, app: &App) {
    let area = frame.area();
    // Conditionally hide ASCII logo on small terminal heights
    let logo = if area.height >= 22 {
        logo_lines_for_area(area.width, 5)
    } else {
        Vec::new()
    };
    let logo_height = if logo.is_empty() { 0 } else { logo.len() as u16 + 1 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(logo_height),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area);

    if logo_height > 0 {
        frame.render_widget(Paragraph::new(logo).alignment(Alignment::Center), chunks[0]);
    }

    let api_key = if app.setup.api_key.is_empty() {
        "not set".to_owned()
    } else {
        let count = app.setup.api_key.chars().count();
        format!("{} ({} chars)", "*".repeat(count.min(12)), count)
    };

    // Split chunks[1] into fields area and tip area
    let has_tip = app.setup.api_key.starts_with("sk-");
    let settings_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Min(8),
            Constraint::Length(if has_tip { 2 } else { 0 }),
        ])
        .split(chunks[1]);

    // Center settings block horizontally inside a 68-char panel
    let centered_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(68),
            Constraint::Min(0),
        ])
        .split(settings_layout[0]);
    let settings_area = centered_layout[1];

    let footer_text = if app.setup.is_editing {
        " █ Enter: Save field • Esc: Cancel editing "
    } else {
        " ▲/▼: Move • Enter/Space: Edit/Toggle • ◀/▶: Cycle options • Esc: Exit "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if app.setup.is_editing {
            Style::default().fg(Color::Rgb(16, 185, 129)).add_modifier(Modifier::BOLD) // Emerald Green when editing
        } else {
            Style::default().fg(Color::Rgb(59, 130, 246)).add_modifier(Modifier::BOLD) // Vibrant Blue normally
        })
        .title(Span::styled(
            format!(" {} DarwinCode Assistant Settings ", icons::SETTINGS_MODE),
            Style::default().add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            footer_text,
            Style::default().fg(Color::DarkGray),
        ))
        .padding(Padding::new(2, 2, 1, 1));

    let inner_area = block.inner(settings_area);

    let fields = vec![
        (
            "API Key",
            api_key,
            SetupField::ApiKey,
            Color::Rgb(236, 72, 153),
        ),
        (
            "Model",
            app.setup.model.clone(),
            SetupField::Model,
            Color::Rgb(168, 85, 247),
        ),
        (
            "Base URL",
            app.setup.base_url.clone(),
            SetupField::BaseUrl,
            Color::Rgb(59, 130, 246),
        ),
        (
            "Codebase Tools",
            (if app.setup.enable_codebase_tools { icons::CHECK_ENABLED } else { icons::CROSS_DISABLED }).to_owned(),
            SetupField::EnableCodebase,
            Color::Rgb(16, 185, 129),
        ),
        (
            "Bash Execution",
            (if app.setup.enable_bash_tools { icons::CHECK_ENABLED } else { icons::CROSS_DISABLED }).to_owned(),
            SetupField::EnableBash,
            Color::Rgb(245, 158, 11),
        ),
        (
            "Security Mode",
            app.setup.permission_level.label().to_owned(),
            SetupField::PermissionLevel,
            Color::Rgb(239, 68, 68),
        ),
        (
            "Thoughts View",
            (if app.setup.show_thoughts { icons::CHECK_SHOW_FULL } else { icons::CROSS_LABEL_ONLY }).to_owned(),
            SetupField::ShowThoughts,
            Color::Rgb(6, 182, 212),
        ),
        (
            "Theme",
            app.setup.theme.label().to_owned(),
            SetupField::Theme,
            Color::Rgb(251, 146, 60),
        ),
        (
            "Respect .gitignore",
            (if app.setup.respect_gitignore { icons::CHECK_ENABLED } else { icons::CROSS_DISABLED }).to_owned(),
            SetupField::RespectGitignore,
            Color::Rgb(168, 85, 247),
        ),
    ];

    let mut all_lines = Vec::new();
    for (label, value, field, color) in fields {
        let is_active = app.setup.active_field == field;
        all_lines.push(draw_setup_field(label, &value, is_active, app.setup.is_editing, color));
    }

    let save_active = app.setup.active_field == SetupField::Save;
    let save_marker = if save_active { icons::ACTIVE_MARKER } else { icons::INACTIVE_MARKER };
    let save_marker_style = if save_active {
        Style::default().fg(Color::Rgb(59, 130, 246)).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
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

    all_lines.push(Line::from(vec![
        Span::styled(save_marker, save_marker_style),
        Span::styled(save_text, save_style),
    ]));

    let viewport_height = inner_area.height as usize;
    let total_lines = all_lines.len();

    let active_line_idx = app.setup.active_field.index();

    let max_scroll = total_lines.saturating_sub(viewport_height);
    let mut offset = 0;
    if active_line_idx >= viewport_height {
        offset = active_line_idx + 1 - viewport_height;
    }
    let scroll_offset = offset.min(max_scroll);

    let end_idx = (scroll_offset + viewport_height).min(total_lines);
    let visible_lines = all_lines[scroll_offset..end_idx].to_vec();

    frame.render_widget(block, settings_area);
    frame.render_widget(
        Paragraph::new(visible_lines),
        inner_area,
    );

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

    render_statusbar(frame, app, chunks[2]);
}

fn draw_setup_field(
    label: &str,
    value: &str,
    active: bool,
    is_editing: bool,
    color: Color,
) -> Line<'static> {
    let marker = if active { icons::ACTIVE_MARKER } else { icons::INACTIVE_MARKER };
    let marker_style = if active {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    // Active label is bold default terminal text. Inactive is DarkGray.
    let label_style = if active {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    // Active value uses the theme accent color. Inactive uses default terminal text.
    let value_style = if active {
        if is_editing {
            Style::default().fg(Color::Rgb(16, 185, 129)).add_modifier(Modifier::BOLD) // Emerald Green when typing!
        } else {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        }
    } else {
        Style::default()
    };

    let mut value_str = value.to_owned();
    if active && is_editing {
        value_str.push('█');
    }

    Line::from(vec![
        Span::styled(marker, marker_style),
        Span::styled(format!("{:<26}", label), label_style),
        Span::styled(value_str, value_style),
    ])
}
