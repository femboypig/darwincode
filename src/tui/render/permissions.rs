use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Padding};

use crate::app::App;
use crate::tui::render::chat::render_messages;
use crate::tui::render::render_statusbar;

pub(crate) fn render_permissions(frame: &mut Frame, app: &App) {
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
