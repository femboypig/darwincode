use crate::app::App;
use crate::tui::render::logo::logo_lines_for_area;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(crate) fn render_welcome_logo(frame: &mut Frame, app: &App, logo_area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let logo_fg = if active_theme.is_light {
        Color::Black
    } else {
        Color::White
    };
    let logo = logo_lines_for_area(Style::default().fg(logo_fg), logo_area.width, 5);
    frame.render_widget(Paragraph::new(logo).alignment(Alignment::Center), logo_area);
}

pub(crate) fn render_welcome_tips(frame: &mut Frame, app: &App, tips_area: Rect) {
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
