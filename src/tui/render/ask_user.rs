use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap};

use crate::app::App;
use crate::tui::render::chat::render_chat;

pub(crate) fn render_ask_user(frame: &mut Frame, app: &App) {
    render_chat(frame, app);

    let area = frame.area();
    
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(14),
            Constraint::Min(0),
        ])
        .split(area);
        
    let popup_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(60),
            Constraint::Min(0),
        ])
        .split(popup_layout[1])[1];

    frame.render_widget(ratatui::widgets::Clear, popup_area);

    let block = Block::default()
        .title("  Clarification Required ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(245, 158, 11)).add_modifier(Modifier::BOLD))
        .padding(Padding::uniform(1));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let question_paragraph = Paragraph::new(app.ask_user.question.as_str())
        .wrap(Wrap { trim: true })
        .alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(question_paragraph, chunks[0]);

    if app.ask_user.is_custom {
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(59, 130, 246)))
            .title(" Write custom response ");
        let input_inner = input_block.inner(chunks[1]);
        frame.render_widget(input_block, chunks[1]);
        
        let input_text = format!("{}█", app.ask_user.custom_input);
        frame.render_widget(Paragraph::new(input_text), input_inner);
    } else {
        let mut list_items = Vec::new();
        for (idx, opt) in app.ask_user.options.iter().enumerate() {
            let is_selected = idx == app.ask_user.selected_idx;
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Rgb(245, 158, 11)).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if is_selected { "● " } else { "○ " };
            list_items.push(ratatui::widgets::ListItem::new(format!("{}{}", prefix, opt)).style(style));
        }
        
        let custom_selected = app.ask_user.selected_idx == app.ask_user.options.len();
        let custom_style = if custom_selected {
            Style::default().fg(Color::Black).bg(Color::Rgb(245, 158, 11)).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let custom_prefix = if custom_selected { "● " } else { "○ " };
        list_items.push(ratatui::widgets::ListItem::new(format!("{}{}", custom_prefix, "Write custom response...")).style(custom_style));

        let list = ratatui::widgets::List::new(list_items);
        frame.render_widget(list, chunks[1]);
    }

    let footer_text = if app.ask_user.is_custom {
        " Enter: Submit answer • Esc: Go back to list "
    } else {
        " ↑/▼: Navigate • Enter: Select option • Ctrl+C: Cancel "
    };
    let footer = Paragraph::new(footer_text)
        .alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[2]);
}
