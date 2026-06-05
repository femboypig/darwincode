use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::tui::render::chat::centered_rect;

/// Called from render_chat when Screen::Sessions — renders popup over dimmed chat bg.
pub(crate) fn render_sessions_popup(frame: &mut Frame, app: &App, area: Rect) {
    let active_theme = crate::tui::render::get_active_theme(app);
    let modal_bg = active_theme
        .background_panel
        .unwrap_or(Color::Rgb(24, 24, 24));
    let modal_fg = active_theme.text;
    let dim_text = active_theme.text_muted;
    let select_bg = active_theme.accent;
    let selected_fg = active_theme.background.unwrap_or(Color::Rgb(20, 20, 20));

    let popup_area = centered_rect(55, 60, area);
    frame.render_widget(ratatui::widgets::Clear, popup_area);
    frame.render_widget(
        Block::default().style(Style::default().bg(modal_bg)),
        popup_area,
    );

    let margin = 1u16;
    let content = Rect {
        x: popup_area.x + margin,
        y: popup_area.y + margin,
        width: popup_area.width.saturating_sub(margin * 2),
        height: popup_area.height.saturating_sub(margin * 2),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Search
            Constraint::Length(1), // Separator line
            Constraint::Min(1),    // List
            Constraint::Length(1), // Separator line
            Constraint::Length(1), // Footer
        ])
        .split(content);

    // Title row
    let title_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(5)])
        .split(chunks[0]);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Saved Sessions",
            Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
        )),
        title_cols[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled("esc", Style::default().fg(dim_text)))
            .alignment(Alignment::Right),
        title_cols[1],
    );

    // Search row
    let search_line = if app.sessions.query.is_empty() {
        Line::from(vec![
            Span::styled(
                "Search: ",
                Style::default().fg(dim_text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "type to filter...",
                Style::default().fg(dim_text).add_modifier(Modifier::ITALIC),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                "Search: ",
                Style::default().fg(dim_text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.sessions.query.clone(), Style::default().fg(modal_fg)),
            Span::styled("█", Style::default().fg(modal_fg)),
        ])
    };
    frame.render_widget(Paragraph::new(search_line), chunks[1]);

    // Position cursor
    let cursor_x = chunks[1].x + 8 + app.sessions.query.chars().count() as u16;
    if cursor_x < chunks[1].right() {
        frame.set_cursor_position((cursor_x, chunks[1].y));
    }

    // Header separator
    frame.render_widget(
        Paragraph::new("─".repeat(content.width as usize)).style(Style::default().fg(dim_text)),
        chunks[2],
    );

    // List
    let list_area = chunks[3];
    let filtered = app.sessions.filtered_sessions();
    let total = filtered.len();

    if total == 0 {
        let msg = if app.sessions.query.is_empty() {
            "No saved sessions"
        } else {
            "No matches found"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(dim_text))),
            list_area,
        );
    } else {
        let selected = app.sessions.selected.min(total.saturating_sub(1));
        let visible = list_area.height as usize;
        let start = if total <= visible || selected < visible / 2 {
            0
        } else if selected >= total - visible / 2 {
            total - visible
        } else {
            selected - visible / 2
        };
        let count = visible.min(total.saturating_sub(start));

        for offset in 0..count {
            let idx = start + offset;
            let session = &filtered[idx];
            let is_sel = idx == selected;
            let row = Rect {
                x: list_area.x,
                y: list_area.y + offset as u16,
                width: list_area.width,
                height: 1,
            };
            if is_sel {
                frame.render_widget(Block::default().style(Style::default().bg(select_bg)), row);
            }
            let id_style = if is_sel {
                Style::default()
                    .bg(select_bg)
                    .fg(selected_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(modal_fg)
            };
            let snip_style = if is_sel {
                Style::default().bg(select_bg).fg(selected_fg)
            } else {
                Style::default().fg(dim_text)
            };

            let id = &session.id;
            let id_len = (id.chars().count() as u16).min(list_area.width.saturating_sub(4));
            let avail = (list_area.width as usize).saturating_sub(id_len as usize + 2);
            let snip: String = session.snippet.chars().take(avail).collect();

            let row_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(id_len),
                    Constraint::Length(2),
                    Constraint::Min(1),
                ])
                .split(row);

            frame.render_widget(
                Paragraph::new(Span::styled(id.as_str(), id_style)),
                row_cols[0],
            );
            frame.render_widget(Paragraph::new(Span::styled("  ", snip_style)), row_cols[1]);
            frame.render_widget(Paragraph::new(Span::styled(snip, snip_style)), row_cols[2]);
        }
    }

    // Footer separator
    frame.render_widget(
        Paragraph::new("─".repeat(content.width as usize)).style(Style::default().fg(dim_text)),
        chunks[4],
    );

    // Footer
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
            Span::styled("resume  ", Style::default().fg(dim_text)),
            Span::styled(
                "Esc ",
                Style::default().fg(modal_fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled("cancel", Style::default().fg(dim_text)),
        ])),
        chunks[5],
    );
}

/// Legacy entry point — now sessions routes through render_chat.
#[allow(dead_code)]
pub(crate) fn render_sessions(frame: &mut Frame, app: &App) {
    crate::tui::render::chat::render_chat(frame, app);
}
