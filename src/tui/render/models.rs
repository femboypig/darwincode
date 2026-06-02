use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding};

use crate::app::App;
use crate::tui::render::render_statusbar;

pub(crate) fn render_models(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(area);

    let items = app
        .models
        .models
        .iter()
        .enumerate()
        .map(|(index, model): (usize, &String)| {
            let model = model.trim_start_matches("models/");
            if index == app.models.selected {
                ListItem::new(Line::from(vec![
                    Span::styled("> ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        model.to_owned(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]))
            } else {
                ListItem::new(format!("  {model}"))
            }
        })
        .collect::<Vec<_>>();

    let mut state = ListState::default().with_selected(Some(app.models.selected));
    keep_selected_visible(
        &mut state,
        app.models.selected,
        chunks[0].height.saturating_sub(2),
    );

    frame.render_stateful_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Models ")
                .padding(Padding::horizontal(1)),
        ),
        chunks[0],
        &mut state,
    );

    render_statusbar(frame, app, chunks[1]);
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
