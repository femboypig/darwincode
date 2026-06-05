use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::tui::render::{get_theme, render_statusbar};

#[allow(dead_code)]
pub(crate) fn render_permissions(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let theme = get_theme(app);
    let modal_fg = match theme {
        crate::config::Theme::Light => Color::Rgb(30, 30, 30),
        _ => Color::Rgb(220, 220, 220),
    };
    let dim_text = match theme {
        crate::config::Theme::Light => Color::Rgb(140, 140, 140),
        _ => Color::Rgb(110, 110, 110),
    };
    let selected_color = match theme {
        crate::config::Theme::Light => Color::Rgb(190, 60, 100),
        _ => Color::Rgb(236, 72, 153),
    };
    let dim_overlay = match theme {
        crate::config::Theme::Light => Color::Rgb(200, 200, 205),
        _ => Color::Rgb(10, 10, 13),
    };
    let modal_bg = match theme {
        crate::config::Theme::Light => Color::Rgb(240, 240, 240),
        _ => Color::Rgb(20, 20, 23),
    };

    // Dim overlay then content (same pattern as ask)
    frame.render_widget(
        Block::default().style(Style::default().bg(dim_overlay)),
        area,
    );
    frame.render_widget(Block::default().style(Style::default().bg(modal_bg)), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let main_area = chunks[0];
    let inner = Rect {
        x: main_area.x + 4,
        y: main_area.y + 1,
        width: main_area.width.saturating_sub(8),
        height: main_area.height.saturating_sub(2),
    };

    if inner.height == 0 || inner.width == 0 {
        render_statusbar(frame, app, chunks[1]);
        return;
    }

    let options = crate::app::PermissionPickerState::options();
    let total = options.len();
    let selected = app.permissions.selected.min(total.saturating_sub(1));

    // Title line: "Select permission level"
    let title_row = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Select permission level",
            Style::default().fg(modal_fg),
        )),
        title_row,
    );

    // Options: 2 lines each (label + description)
    let list_start_y = inner.y + 2; // +1 title + 1 blank

    for idx in 0..total {
        let (label, desc, _) = options[idx];
        let is_selected = idx == selected;

        let label_y = list_start_y + (idx as u16 * 2);
        if label_y >= inner.bottom() {
            break;
        }

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

        let label_row = Rect {
            x: inner.x,
            y: label_y,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{}.", idx + 1), num_style),
                Span::styled("  ", Style::default()),
                Span::styled(label, label_style),
            ])),
            label_row,
        );

        let desc_y = label_y + 1;
        if desc_y < inner.bottom() {
            let desc_row = Rect {
                x: inner.x,
                y: desc_y,
                width: inner.width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(desc, Style::default().fg(dim_text)),
                ])),
                desc_row,
            );
        }
    }

    // Footer
    let footer_y = inner.bottom().saturating_sub(1);
    if footer_y > list_start_y {
        let footer_row = Rect {
            x: inner.x,
            y: footer_y,
            width: inner.width,
            height: 1,
        };
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
            footer_row,
        );
    }

    render_statusbar(frame, app, chunks[1]);
}
