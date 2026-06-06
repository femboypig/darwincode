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
    if app.ui.theme_picker_open
        && let Some(theme) = app.ui.theme_picker.selected_theme()
    {
        crate::config::resolve_theme(&theme)
    } else {
        let raw_theme = if app.ui.screen == Screen::Setup {
            &app.ui.setup.theme
        } else {
            &app.chat.config.theme
        };
        crate::config::resolve_theme(raw_theme)
    }
}

pub(crate) fn get_active_theme(app: &App) -> crate::tui::theme::ActiveTheme {
    let temp_theme;
    let raw_theme = if app.ui.theme_picker_open
        && let Some(theme) = app.ui.theme_picker.selected_theme()
    {
        temp_theme = theme;
        &temp_theme
    } else if app.ui.screen == Screen::Setup {
        &app.ui.setup.theme
    } else {
        &app.chat.config.theme
    };

    let mode = crate::config::resolve_theme_mode(raw_theme);

    match raw_theme {
        crate::config::Theme::Custom(name) => {
            if let Some(config) = crate::tui::theme::custom_themes().get(name) {
                config.resolve(&mode)
            } else {
                match mode {
                    crate::tui::theme::ThemeMode::Dark => crate::tui::theme::ActiveTheme::default(),
                    crate::tui::theme::ThemeMode::Light => {
                        crate::tui::theme::ActiveTheme::light_default()
                    }
                }
            }
        }
        _ => match mode {
            crate::tui::theme::ThemeMode::Dark => crate::tui::theme::ActiveTheme::default(),
            crate::tui::theme::ThemeMode::Light => crate::tui::theme::ActiveTheme::light_default(),
        },
    }
}

pub(crate) fn render(frame: &mut Frame, app: &App) {
    frame.render_widget(ratatui::widgets::Clear, frame.area());
    let active_theme = get_active_theme(app);
    let bg_color = active_theme.background.unwrap_or(Color::Rgb(15, 15, 15));
    frame.render_widget(
        Block::default().style(Style::default().bg(bg_color)),
        frame.area(),
    );

    match app.ui.screen {
        Screen::Setup => chat::render_chat(frame, app),
        Screen::Chat | Screen::Permissions | Screen::Sessions => chat::render_chat(frame, app),
        Screen::AskUser => chat::render_chat(frame, app),
    }
}

pub(crate) fn render_statusbar(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = get_active_theme(app);
    let bar_bg = active_theme.background.unwrap_or(Color::Rgb(15, 15, 15));
    let bar_fg = active_theme.text_muted;
    let base_style = Style::default().bg(bar_bg).fg(bar_fg);

    // Fill the entire status bar background first
    frame.render_widget(Block::default().style(base_style), area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),
            Constraint::Percentage(50),
            Constraint::Length(25),
        ])
        .split(area);

    let dir = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());

    let left_text = format!("  {} ", dir);
    let left_paragraph = Paragraph::new(left_text)
        .alignment(Alignment::Left)
        .style(base_style.add_modifier(Modifier::BOLD));
    frame.render_widget(left_paragraph, chunks[0]);

    if let Some(ref warning) = app.last_warning {
        let warning_text = format!("⚠️  {}", warning);
        let warning_paragraph = Paragraph::new(warning_text)
            .alignment(Alignment::Center)
            .style(base_style.fg(Color::Rgb(245, 158, 11)).add_modifier(Modifier::BOLD));
        frame.render_widget(warning_paragraph, chunks[1]);
    }

    let version = env!("CARGO_PKG_VERSION");
    let right_text = format!("darwincode {}  ", version);
    let right_paragraph = Paragraph::new(right_text)
        .alignment(Alignment::Right)
        .style(base_style);
    frame.render_widget(right_paragraph, chunks[2]);
}
