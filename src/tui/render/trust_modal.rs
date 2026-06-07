use crate::app::App;
use crate::tui::render::icons::icons;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

pub(crate) fn render_trust_modal(frame: &mut Frame, app: &App) {
    if !app.ui.show_trust_modal {
        return;
    }
    let active_theme = crate::tui::render::get_active_theme(app);
    let full = frame.area();

    dim_background(frame, full, &active_theme);

    let popup = centered_rect(72, 64, full);

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
        .title_alignment(Alignment::Center)
        .style(
            Style::default().bg(active_theme
                .background_panel
                .unwrap_or(Color::Rgb(24, 24, 24))),
        );
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let path_display = app
        .ui
        .trust_modal_proj_path
        .clone()
        .unwrap_or_else(|| "(unknown path)".to_owned());

    let body_lines = vec![
        Line::from(Span::styled(
            "Trust this project?",
            Style::default()
                .fg(active_theme.text)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Workspace:",
            Style::default().fg(active_theme.text_muted),
        )),
        Line::from(Span::styled(
            format!("  {}", path_display),
            Style::default()
                .fg(active_theme.primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "When you trust a workspace:",
            Style::default()
                .fg(active_theme.text)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  • Custom slash commands in .darwincode/commands/ run without further confirmation",
            Style::default().fg(active_theme.text),
        )),
        Line::from(Span::styled(
            "  • Custom agents in .darwincode/agents/ may invoke any tool in their allow-list",
            Style::default().fg(active_theme.text),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "When you don't:",
            Style::default()
                .fg(active_theme.text)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  • darwincode will pop a y/n confirmation before every shell action from .darwincode/",
            Style::default().fg(active_theme.text),
        )),
        Line::from(Span::styled(
            "  • You can switch to trust later with /settings → Trust Workspace",
            Style::default().fg(active_theme.text_muted),
        )),
    ];
    let body = Paragraph::new(body_lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(active_theme.text));
    frame.render_widget(body, inner_chunks[1]);

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
        Span::raw("    "),
        Span::styled("  Y  Yes, trust this workspace  ", yes_style),
        Span::raw("    "),
        Span::styled("  N  No, keep asking  ", no_style),
        Span::raw("    "),
    ]);
    frame.render_widget(
        Paragraph::new(buttons).alignment(Alignment::Center),
        inner_chunks[3],
    );

    let hint = Paragraph::new(Span::styled(
        "← / →  switch  •  Enter  confirm  •  Esc  dismiss without saving",
        Style::default().fg(active_theme.text_muted),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(hint, inner_chunks[4]);
}

fn dim_background(frame: &mut Frame, area: Rect, theme: &crate::tui::theme::ActiveTheme) {
    let scrim_bg = if theme.is_light {
        Color::Rgb(60, 60, 60)
    } else {
        Color::Rgb(10, 10, 10)
    };
    let mut scrim = String::with_capacity(area.width as usize);
    for _ in 0..area.width {
        scrim.push(' ');
    }
    let lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(Span::styled(scrim.as_str(), Style::default().bg(scrim_bg))))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
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
