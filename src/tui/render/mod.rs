pub(crate) mod icons;
pub(crate) mod logo;
pub(crate) mod setup;
pub(crate) mod chat;
pub(crate) mod models;
pub(crate) mod permissions;
pub(crate) mod sessions;
pub(crate) mod ask_user;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Screen};

pub(crate) fn get_theme(app: &App) -> crate::config::Theme {
    let raw_theme = if app.screen == Screen::Setup {
        app.setup.theme
    } else {
        app.chat.config.theme
    };
    crate::config::resolve_theme(raw_theme)
}

pub(crate) fn render(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Setup => setup::render_setup(frame, app),
        Screen::Chat => chat::render_chat(frame, app),
        Screen::Models => models::render_models(frame, app),
        Screen::Permissions => permissions::render_permissions(frame, app),
        Screen::Sessions => sessions::render_sessions(frame, app),
        Screen::AskUser => ask_user::render_ask_user(frame, app),
    }
}

pub(crate) fn render_statusbar(frame: &mut Frame, app: &App, area: Rect) {
    use icons::icons;
    let (mode_text, mode_bg, mode_fg) = match app.screen {
        Screen::Chat => (icons::CHAT_MODE, Color::Rgb(59, 130, 246), Color::Black), // Vibrant blue
        Screen::Setup => (icons::SETTINGS_MODE, Color::Rgb(236, 72, 153), Color::Black), // Magenta/Pink
        Screen::Models => (icons::MODELS_MODE, Color::Rgb(168, 85, 247), Color::Black), // Purple
        Screen::Permissions => (icons::SECURITY_MODE, Color::Rgb(245, 158, 11), Color::Black), // Amber/Yellow
        Screen::Sessions => (icons::SESSIONS_MODE, Color::Rgb(16, 185, 129), Color::Black), // Emerald green
        Screen::AskUser => (icons::ASK_USER_MODE, Color::Rgb(245, 158, 11), Color::Black), // Amber/Yellow
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
