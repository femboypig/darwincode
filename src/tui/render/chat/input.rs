use crate::app::App;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem};

pub(crate) fn render_command_suggestions(frame: &mut Frame, app: &App, area: Rect) {
    let suggestions = app.command_suggestions();
    if suggestions.is_empty() {
        return;
    }

    let active_theme = crate::tui::render::get_active_theme(app);
    let bg_color = active_theme
        .background_panel
        .unwrap_or(Color::Rgb(24, 24, 24));

    let border_color = if app.chat.shell_focused {
        Color::DarkGray
    } else {
        Color::Rgb(59, 130, 246)
    };

    let mut line_lines = Vec::new();
    for _ in 0..area.height {
        line_lines.push(Line::from(Span::styled(
            "┃",
            Style::default().fg(border_color),
        )));
    }
    frame.render_widget(
        Paragraph::new(line_lines),
        Rect {
            x: area.x,
            y: area.y,
            width: 1,
            height: area.height,
        },
    );

    let list_area = Rect {
        x: area.x.saturating_add(1),
        y: area.y,
        width: area.width.saturating_sub(1),
        height: area.height,
    };

    // Draw background for suggestions box to match prompt background color
    frame.render_widget(
        Block::default().style(Style::default().bg(bg_color)),
        list_area,
    );

    let total_len = suggestions.len();
    let window_size = 10;
    let selected_idx = app.chat.suggestion_idx.min(total_len.saturating_sub(1));

    let start_idx = if total_len <= window_size || selected_idx < window_size / 2 {
        0
    } else if selected_idx >= total_len - window_size / 2 {
        total_len - window_size
    } else {
        selected_idx - window_size / 2
    };

    let visible_suggestions: Vec<_> = suggestions
        .into_iter()
        .skip(start_idx)
        .take(window_size)
        .collect();

    let max_name_len = visible_suggestions
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(0);

    let mut items = Vec::new();
    for (idx, suggestion) in visible_suggestions.into_iter().enumerate() {
        let global_idx = start_idx + idx;
        let is_active = global_idx == selected_idx;
        let name_len = suggestion.name.chars().count();
        let padding_spaces = " ".repeat(max_name_len.saturating_sub(name_len) + 4);

        let line = if is_active {
            Line::from(vec![
                Span::styled(
                    format!(" {}", suggestion.name),
                    Style::default()
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(padding_spaces, Style::default().fg(Color::Black)),
                Span::styled(
                    suggestion.description,
                    Style::default().fg(Color::Rgb(60, 60, 60)),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    format!(" {}", suggestion.name),
                    Style::default()
                        .fg(active_theme.text)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(padding_spaces, Style::default().fg(active_theme.text_muted)),
                Span::styled(
                    suggestion.description,
                    Style::default().fg(active_theme.text_muted),
                ),
            ])
        };

        let item_style = if is_active {
            Style::default().bg(active_theme.accent).fg(Color::Black)
        } else {
            Style::default()
        };

        items.push(ListItem::new(line).style(item_style));
    }

    let fg_color = active_theme.text;
    let block = Block::default();
    let list_widget = List::new(items)
        .block(block)
        .style(Style::default().bg(bg_color).fg(fg_color));
    frame.render_widget(list_widget, list_area);
}

use ratatui::widgets::Paragraph;
