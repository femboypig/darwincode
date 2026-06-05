pub(crate) mod chat;
pub(crate) mod icons;
pub(crate) mod logo;
pub(crate) mod permissions;
pub(crate) mod sessions;
pub(crate) mod setup;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Paragraph};

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
    frame.render_widget(ratatui::widgets::Clear, frame.area());
    let theme = get_theme(app);
    let bg_color = match theme {
        crate::config::Theme::Light => Color::Rgb(248, 248, 248),
        _ => Color::Rgb(15, 15, 15),
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(bg_color)),
        frame.area(),
    );

    match app.screen {
        Screen::Setup => chat::render_chat(frame, app),
        Screen::Chat | Screen::Permissions | Screen::Sessions => chat::render_chat(frame, app),
        Screen::AskUser => chat::render_chat(frame, app),
    }
}

pub(crate) fn render_statusbar(frame: &mut Frame, app: &App, area: Rect) {
    // todo: rework this shit
    let theme = get_theme(app);
    let bg_color = match theme {
        crate::config::Theme::Light => Color::Rgb(248, 248, 248),
        _ => Color::Rgb(15, 15, 15),
    };
    let (bar_bg, bar_fg) = match theme {
        crate::config::Theme::Light => (bg_color, Color::Rgb(80, 80, 80)),
        _ => (bg_color, Color::Rgb(160, 160, 160)),
    };
    let base_style = Style::default().bg(bar_bg).fg(bar_fg);

    // Fill the entire status bar background first
    frame.render_widget(Block::default().style(base_style), area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(30)])
        .split(area);

    let dir = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());

    let left_text = format!("  {} ", dir);
    let left_paragraph = Paragraph::new(left_text)
        .alignment(Alignment::Left)
        .style(base_style.add_modifier(Modifier::BOLD));
    frame.render_widget(left_paragraph, chunks[0]);

    let version = env!("CARGO_PKG_VERSION");
    let right_text = format!("darwincode {}  ", version);
    let right_paragraph = Paragraph::new(right_text)
        .alignment(Alignment::Right)
        .style(base_style);
    frame.render_widget(right_paragraph, chunks[1]);
}
