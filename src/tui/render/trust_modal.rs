use crate::app::App;
use crate::tui::render::icons::icons;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

/// Render the trust-this-workspace modal as an overlay on top of the chat
/// screen. Drawn last so it sits on top of any other content.
pub(crate) fn render_trust_modal(frame: &mut Frame, app: &App) {
    if !app.ui.show_trust_modal {
        return;
    }
    let active_theme = crate::tui::render::get_active_theme(app);
    let area = frame.area();

    // Centered popup: 70% width, ~30% height
    let popup = centered_rect(70, 36, area);

    frame.render_widget(Clear, popup);
    let border_color = if app.ui.trust_modal_selected_yes {
        active_theme.warning
    } else {
        active_theme.primary
    };
    let block = Block::bordered()
        .border_style(Style::default().fg(border_color))
        .title(Line::from(Span::styled(
            format!(" {} Trust Workspace ", icons::WARNING),
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        )))
        .title_alignment(Alignment::Center);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let path_display = app
        .ui
        .trust_modal_proj_path
        .clone()
        .unwrap_or_else(|| "(unknown path)".to_owned());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacer
            Constraint::Length(3), // body
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Length(1), // footer hint
        ])
        .split(inner);

    let body = Paragraph::new(vec![
        Line::from(Span::styled(
            "darwincode detected a project in this workspace:",
            Style::default().fg(active_theme.text),
        )),
        Line::from(Span::styled(
            path_display,
            Style::default()
                .fg(active_theme.primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Custom commands and agents in .darwincode/ may execute shell actions on your behalf.",
            Style::default().fg(active_theme.text_muted),
        )),
    ])
    .wrap(Wrap { trim: false });
    frame.render_widget(body, chunks[1]);

    let yes_style = if app.ui.trust_modal_selected_yes {
        Style::default()
            .bg(active_theme.success)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(active_theme.text_muted)
    };
    let no_style = if app.ui.trust_modal_selected_yes {
        Style::default().fg(active_theme.text_muted)
    } else {
        Style::default()
            .bg(active_theme.error)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    };

    let buttons = Line::from(vec![
        Span::raw("   "),
        Span::styled(" [Y] Yes, trust ", yes_style),
        Span::raw("    "),
        Span::styled(" [N] No, keep asking ", no_style),
        Span::raw("   "),
    ]);
    frame.render_widget(
        Paragraph::new(buttons).alignment(Alignment::Center),
        chunks[3],
    );

    let hint = Paragraph::new(Span::styled(
        "←/→ to switch • Enter to confirm • Esc to dismiss",
        Style::default().fg(active_theme.text_muted),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[4]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
