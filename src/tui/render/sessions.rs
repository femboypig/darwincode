use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph};

use crate::app::App;
use crate::tui::render::{get_theme, render_statusbar};

pub(crate) fn render_sessions(frame: &mut Frame, app: &App) {
    let theme = get_theme(app);
    let (label_color, query_color) = match theme {
        crate::config::Theme::Light => (Color::Blue, Color::Black),
        _ => (Color::Cyan, Color::White),
    };

    let area = frame.area();
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(area);

    let session_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(main_chunks[0]);

    let filter_text = format!(" {}", app.sessions.query);
    let filter_para = Paragraph::new(Line::from(vec![
        Span::styled(" Search: ", Style::default().fg(label_color).add_modifier(Modifier::BOLD)),
        Span::styled(filter_text, Style::default().fg(query_color)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Filter Sessions ")
    );
    frame.render_widget(filter_para, session_chunks[0]);

    let filtered = app.sessions.filtered_sessions();
    let (sel_bg, sel_fg_id, sel_fg_snippet) = match theme {
        crate::config::Theme::Light => (Color::Rgb(220, 220, 230), Color::Black, Color::Black),
        _ => (Color::Rgb(40, 40, 50), Color::White, Color::White),
    };
    let (unsel_fg_id, unsel_fg_snippet) = match theme {
        crate::config::Theme::Light => (Color::Rgb(50, 50, 50), Color::Rgb(100, 100, 100)),
        _ => (Color::Gray, Color::DarkGray),
    };

    let items = filtered
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let id = &session.id;
            let snippet = &session.snippet;
            
            if index == app.sessions.selected {
                ListItem::new(Line::from(vec![
                    Span::styled("> ", Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow)),
                    Span::styled(format!("{:<30} ", id), Style::default().add_modifier(Modifier::BOLD).fg(sel_fg_id)),
                    Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(snippet.clone(), Style::default().fg(sel_fg_snippet)),
                ])).style(Style::default().bg(sel_bg))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<30} ", id), Style::default().fg(unsel_fg_id)),
                    Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(snippet.clone(), Style::default().fg(unsel_fg_snippet)),
                ]))
            }
        })
        .collect::<Vec<_>>();

    let selected_opt = if filtered.is_empty() { None } else { Some(app.sessions.selected) };
    let mut state = ListState::default().with_selected(selected_opt);
    if !filtered.is_empty() {
        keep_selected_visible(
            &mut state,
            app.sessions.selected,
            session_chunks[1].height.saturating_sub(2),
        );
    }

    frame.render_stateful_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Resume Chat Session ")
                .padding(Padding::horizontal(1)),
        ),
        session_chunks[1],
        &mut state,
    );

    render_statusbar(frame, app, main_chunks[1]);
}

fn keep_selected_visible(state: &mut ListState, selected: usize, viewport_height: u16) {
    let visible_rows = viewport_height.max(1) as usize;
    let offset = if selected >= visible_rows {
        selected + 1 - visible_rows
    } else {
        0
    };

    *state.offset_mut() = offset;
}
