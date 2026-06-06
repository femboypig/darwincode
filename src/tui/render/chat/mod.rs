pub(crate) mod welcome;
pub(crate) mod message_list;
pub(crate) mod input;
pub(crate) mod panels;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::app::Screen;
use crate::tui::render::logo::{logo_fits, logo_lines};
use crate::tui::render::render_statusbar;
use crate::tui::syntax::wrap_text_to_lines;

use welcome::{render_welcome_logo, render_welcome_tips};
use message_list::{render_messages, dim_buffer};
pub(crate) use message_list::centered_rect;

use input::render_command_suggestions;
use panels::{
    render_model_picker_modal, render_theme_picker_modal, render_agent_picker_modal,
    render_confirm_modal, render_ask_user_in_input_box, render_permissions_in_input_box,
    render_todos,
};

pub(crate) fn render_chat(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let suggestions = app.command_suggestions();
    let suggestion_height = if suggestions.is_empty() {
        0
    } else {
        u16::try_from(suggestions.len().min(10)).unwrap_or(u16::MAX)
    };

    let has_todos = !app.chat.todos.is_empty() && !app.chat.messages.is_empty();
    let (left_pane, right_pane, statusbar_area) = if has_todos && area.width >= 50 {
        let sidebar_width = if area.width >= 90 { 36 } else { 30 };
        let horizontal_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(sidebar_width)])
            .split(area);
        let left_full_area = horizontal_split[0];
        let right_pane = horizontal_split[1];

        let vertical_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1), // Status bar
                Constraint::Length(1), // Bottom spacer (empty line at bottom)
            ])
            .split(left_full_area);

        (vertical_split[0], Some(right_pane), vertical_split[1])
    } else {
        let vertical_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1), // Status bar
                Constraint::Length(1), // Bottom spacer (empty line at bottom)
            ])
            .split(area);
        (vertical_split[0], None, vertical_split[1])
    };

    // Text wrapped for input: padding of 1 on each side inside the text block,
    // plus margin(2) and blue line(1) on the left side of text block,
    // plus margin(2) on the right side of text block. Total horizontal spacing is 7.
    let input_inner_w = if app.chat.messages.is_empty() {
        // Centered welcome input box is 70% of screen width of left_pane
        let centered_w = (left_pane.width as u32 * 70 / 100) as u16;
        centered_w.saturating_sub(7).max(1)
    } else {
        left_pane.width.saturating_sub(7).max(1)
    };
    let wrapped_lines = wrap_text_to_lines(app.chat.input.as_str(), input_inner_w as usize);
    let display_lines = u16::try_from(wrapped_lines.len()).unwrap_or(u16::MAX);
    let input_height = if app.ui.screen == Screen::AskUser {
        let q_wrapped = wrap_text_to_lines(app.ui.ask_user.question.as_str(), input_inner_w as usize);
        let q_lines = u16::try_from(q_wrapped.len()).unwrap_or(u16::MAX);
        let opt_count = u16::try_from(app.ui.ask_user.options.len()).unwrap_or(u16::MAX);
        let custom_lines = if app.ui.ask_user.is_custom {
            if app.ui.ask_user.custom_input.is_empty() {
                1
            } else {
                u16::try_from(app.ui.ask_user.custom_input.split('\n').count()).unwrap_or(u16::MAX)
            }
        } else {
            0
        };
        // q_lines + blank + opts (label only) + custom + 1 blank + 1 footer + 1 blank + 2 padding
        (q_lines + 1 + opt_count + custom_lines + 1 + 1 + 1 + 2).max(6)
    } else if app.ui.screen == Screen::Permissions {
        let opts = crate::app::PermissionPickerState::options();
        let opt_count = u16::try_from(opts.len()).unwrap_or(u16::MAX);
        // title + blank + 2*opts + 1 blank + 1 footer + 2 padding
        (1 + 1 + opt_count * 2 + 1 + 1 + 2).max(6)
    } else {
        // Input block height: display_lines (min 1, max 5) + 1 top + 1 spacer + 1 mode/model + 1 bottom
        display_lines.clamp(1, 5) + 4
    };

    let (messages_area, suggestions_area, queue_area, input_area, logo_area, tips_area) =
        if app.chat.messages.is_empty() {
            let active_theme = crate::tui::render::get_active_theme(app);
            let logo_fg = if active_theme.is_light {
                Color::Black
            } else {
                Color::White
            };
            let logo_lines = logo_lines(Style::default().fg(logo_fg));
            let logo_fits_flag = logo_fits(&logo_lines, left_pane.width, 5);

            let remaining_height = left_pane
                .height
                .saturating_sub(input_height + suggestion_height);
            let half_height = remaining_height / 2;
            let top_height = half_height;
            let bottom_height = remaining_height.saturating_sub(half_height);

            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(top_height),
                    Constraint::Length(suggestion_height),
                    Constraint::Length(input_height),
                    Constraint::Length(bottom_height),
                ])
                .split(left_pane);

            let logo_spacer = if top_height >= 8 {
                3
            } else if top_height >= 7 {
                2
            } else if top_height >= 6 {
                1
            } else {
                0
            };

            let top_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(5), // Logo (5 lines)
                    Constraint::Length(logo_spacer),
                ])
                .split(main_chunks[0]);

            let bottom_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Spacer
                    Constraint::Length(1), // Tips line
                    Constraint::Min(0),
                ])
                .split(main_chunks[3]);

            let centered_suggestions_box = if suggestion_height > 0 {
                Some(
                    Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Percentage(15),
                            Constraint::Percentage(70),
                            Constraint::Percentage(15),
                        ])
                        .split(main_chunks[1])[1],
                )
            } else {
                None
            };

            let centered_input_box = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(15),
                    Constraint::Percentage(70),
                    Constraint::Percentage(15),
                ])
                .split(main_chunks[2])[1];

            let logo_area = if logo_fits_flag && top_height >= 5 {
                Some(top_chunks[1])
            } else {
                None
            };

            let tips_area = if bottom_height >= 2 {
                Some(bottom_chunks[1])
            } else {
                None
            };

            (
                None,
                centered_suggestions_box,
                None,
                Some(centered_input_box),
                logo_area,
                tips_area,
            )
        } else {
            let queue_height = if app.chat.message_queue.is_empty() {
                0
            } else {
                u16::try_from(app.chat.message_queue.len().min(3)).unwrap_or(u16::MAX) + 1
            };

            let spacer_height = if suggestion_height > 0 || queue_height > 0 {
                0
            } else {
                1
            };

            let normal_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(4),
                    Constraint::Length(suggestion_height),
                    Constraint::Length(queue_height),
                    Constraint::Length(spacer_height), // dynamic vertical spacer
                    Constraint::Length(input_height),
                ])
                .split(left_pane);

            (
                Some(normal_chunks[0]),
                if suggestion_height > 0 {
                    Some(normal_chunks[1])
                } else {
                    None
                },
                if queue_height > 0 {
                    Some(normal_chunks[2])
                } else {
                    None
                },
                Some(normal_chunks[4]),
                None,
                None,
            )
        };

    if let Some(logo_area) = logo_area {
        render_welcome_logo(frame, app, logo_area);
    }

    if let Some(tips_area) = tips_area {
        render_welcome_tips(frame, app, tips_area);
    }

    if let Some(messages_area) = messages_area {
        if !app.chat.todos.is_empty() && right_pane.is_none() {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(8)])
                .split(messages_area);
            render_messages(frame, app, layout[0]);
            render_todos(frame, app, layout[1]);
        } else {
            render_messages(frame, app, messages_area);
        }
    }

    if let Some(suggestions_area) = suggestions_area {
        let margin = 2;
        let flat_suggestions_area = Rect {
            x: suggestions_area.x.saturating_add(margin),
            y: suggestions_area.y,
            width: suggestions_area.width.saturating_sub(margin * 2),
            height: suggestions_area.height,
        };
        render_command_suggestions(frame, app, flat_suggestions_area);
    }

    if let Some(queue_area) = queue_area {
        let mut queue_lines = vec![Line::from(Span::styled(
            " Queued Prompts: ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ))];
        for (idx, item) in app.chat.message_queue.iter().enumerate().take(3) {
            let truncated_item = if item.chars().count() > 60 {
                let s: String = item.chars().take(57).collect();
                format!("{}...", s)
            } else {
                item.clone()
            };
            queue_lines.push(Line::from(vec![
                Span::styled(
                    format!("  [{}] ", idx + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(truncated_item, Style::default().fg(Color::DarkGray)),
            ]));
        }
        if app.chat.message_queue.len() > 3 {
            let remaining = app.chat.message_queue.len() - 3;
            queue_lines.push(Line::from(Span::styled(
                format!("  ... and {} more", remaining),
                Style::default().fg(Color::DarkGray),
            )));
        }
        frame.render_widget(Paragraph::new(queue_lines), queue_area);
    }

    let input_box = input_area.unwrap();

    // --- Render input paragraph with scroll ---
    let margin = 2;
    let flat_input_area = Rect {
        x: input_box.x.saturating_add(margin),
        y: input_box.y,
        width: input_box.width.saturating_sub(margin * 2),
        height: input_box.height,
    };

    let active_theme = crate::tui::render::get_active_theme(app);
    let border_color = if app.chat.shell_focused {
        active_theme.text_muted
    } else {
        active_theme.primary
    };

    // Draw the vertical blue line on the left (separated by 1 column)
    let mut blue_line_lines = Vec::new();
    for _ in 0..input_height {
        blue_line_lines.push(Line::from(Span::styled(
            "┃",
            Style::default().fg(border_color),
        )));
    }
    frame.render_widget(
        Paragraph::new(blue_line_lines),
        Rect {
            x: flat_input_area.x,
            y: flat_input_area.y,
            width: 1,
            height: input_height,
        },
    );

    // The text block starts 1 column to the right of flat_input_area.x
    let text_block_area = Rect {
        x: flat_input_area.x.saturating_add(1),
        y: flat_input_area.y,
        width: flat_input_area.width.saturating_sub(1),
        height: input_height,
    };

    let active_theme = crate::tui::render::get_active_theme(app);
    let bg_color = active_theme.background_element.unwrap_or({
        if active_theme.is_light {
            Color::Rgb(240, 240, 240)
        } else {
            Color::Rgb(24, 24, 24)
        }
    });
    let fg_color = active_theme.text;

    // Render background block
    let bg_block = Block::default().style(Style::default().bg(bg_color));
    frame.render_widget(bg_block, text_block_area);

    // Inside the text block, padding is 1.
    let inner_x = text_block_area.x.saturating_add(3);
    let inner_y = if app.ui.screen == Screen::AskUser {
        text_block_area.y
    } else {
        text_block_area.y.saturating_add(1)
    };
    let inner_w = if app.ui.screen == Screen::AskUser {
        text_block_area.width.saturating_sub(4)
    } else {
        text_block_area.width.saturating_sub(5) // match the new inner_x and leave 2 right padding
    };
    let inner_h = if app.ui.screen == Screen::AskUser {
        text_block_area.height
    } else {
        text_block_area.height.saturating_sub(1)
    };

    let text_area = if app.ui.screen == Screen::AskUser {
        Rect {
            x: inner_x,
            y: inner_y,
            width: inner_w,
            height: inner_h,
        }
    } else {
        Rect {
            x: inner_x,
            y: inner_y,
            width: inner_w,
            height: inner_h.saturating_sub(3), // leave 1 spacer + 1 mode/model + 1 bottom padding
        }
    };

    let mode_model_row_area = Rect {
        x: inner_x,
        y: inner_y.saturating_add(inner_h.saturating_sub(2)), // 1 spacer below input, 1 padding below indicator
        width: inner_w,
        height: 1,
    };

    let input_inner_width = text_area.width.max(1);
    let total_visual_lines = display_lines;

    // Compute cursor visual row and column
    let mut cursor_visual_row: u16 = 0;
    let mut cursor_col_in_logical: u16 = 0; // position within current logical line
    for (i, c) in app.chat.input.chars().enumerate() {
        if i == app.chat.cursor {
            break;
        }
        if c == '\n' {
            cursor_visual_row += 1;
            cursor_col_in_logical = 0;
        } else {
            cursor_col_in_logical += 1;
            if cursor_col_in_logical.is_multiple_of(input_inner_width) {
                cursor_visual_row += 1;
            }
        }
    }
    let cursor_x = cursor_col_in_logical % input_inner_width;

    // Auto-scroll: keep cursor visible within the scrollable viewport (text_area.height)
    let max_visible = text_area.height;
    let mut input_scroll = app.chat.input_scroll;
    if cursor_visual_row < input_scroll {
        input_scroll = cursor_visual_row;
    } else if cursor_visual_row >= input_scroll + max_visible {
        input_scroll = cursor_visual_row + 1 - max_visible;
    }
    let max_scroll = total_visual_lines.saturating_sub(max_visible);
    input_scroll = input_scroll.min(max_scroll);

    let paragraph_content: Vec<Line> = if app.chat.input.is_empty() {
        let placeholder = "Ask anything... \"Fix a TODO in the codebase\"";
        let active_theme = crate::tui::render::get_active_theme(app);
        let placeholder_color = active_theme.text_muted;
        vec![Line::from(Span::styled(
            placeholder,
            Style::default().fg(placeholder_color),
        ))]
    } else {
        wrapped_lines.into_iter().map(Line::from).collect()
    };

    if app.ui.screen == Screen::AskUser {
        render_ask_user_in_input_box(frame, app, text_block_area);
    } else if app.ui.screen == Screen::Permissions {
        render_permissions_in_input_box(frame, app, text_block_area);
    } else {
        // Render the scrollable paragraph
        frame.render_widget(
            Paragraph::new(paragraph_content)
                .style(Style::default().fg(fg_color))
                .scroll((input_scroll, 0)),
            text_area,
        );

        // Cursor position: text starts at text_area.x, y starts at text_area.y
        let cursor_y_in_box = cursor_visual_row.saturating_sub(input_scroll);
        let target_y = text_area.y + cursor_y_in_box;
        let max_y = text_area.bottom().saturating_sub(1);

        if !app.chat.shell_focused
            && !app.ui.model_picker_open
            && target_y <= max_y
            && target_y >= text_area.y
        {
            frame.set_cursor_position((text_area.x + cursor_x, target_y));
        }

        // Render the mode/model indicator row inside the bottom area of the block
        let mode_color = match app.core.dev_mode {
            crate::app::DevelopMode::Plan => Color::Rgb(168, 85, 247), // purple
            crate::app::DevelopMode::Build => Color::Rgb(59, 130, 246), // blue (mockup)
        };

        let mode_str = app.dev_mode_label();
        let model_str = app.model_label();
        let mode_len = u16::try_from(mode_str.chars().count()).unwrap_or(u16::MAX);
        let model_len = u16::try_from(model_str.chars().count()).unwrap_or(u16::MAX);

        let mode_rect = Rect {
            x: mode_model_row_area.x,
            y: mode_model_row_area.y,
            width: mode_len,
            height: 1,
        };
        let model_rect = Rect {
            x: mode_model_row_area
                .x
                .saturating_add(mode_len)
                .saturating_add(3), // after mode label + separator " · "
            y: mode_model_row_area.y,
            width: model_len,
            height: 1,
        };
        app.chat.mode_area.set(Some(mode_rect));
        app.chat.model_area.set(Some(model_rect));

        let active_theme = crate::tui::render::get_active_theme(app);
        let separator_style = Style::default().fg(active_theme.text_muted);
        let mut mode_model_spans = vec![
            Span::styled(
                mode_str,
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", separator_style),
        ];

        // Split model label by whitespace: first two words theme text, subsequent words theme text_muted
        for (idx, word) in model_str.split_whitespace().enumerate() {
            if idx > 0 {
                mode_model_spans.push(Span::raw(" "));
            }
            let style = if idx < 2 {
                Style::default().fg(active_theme.text)
            } else {
                Style::default().fg(active_theme.text_muted)
            };
            mode_model_spans.push(Span::styled(word.to_owned(), style));
        }

        frame.render_widget(
            Paragraph::new(Line::from(mode_model_spans)).alignment(Alignment::Left),
            mode_model_row_area,
        );
    } // end else (normal input render)

    if let Some(pane) = right_pane {
        render_todos(frame, app, pane);
    }

    render_statusbar(frame, app, statusbar_area);

    let has_modal = app.ui.model_picker_open
        || app.ui.theme_picker_open
        || app.ui.agent_picker_open
        || app.ui.screen == Screen::Sessions
        || app.ui.screen == Screen::Setup
        || app
            .proc.pending
            .as_ref()
            .is_some_and(|p| matches!(p, crate::app::PendingTask::ConfirmFunction { .. }));

    if has_modal {
        dim_buffer(frame, area);
    }

    if app.ui.model_picker_open {
        render_model_picker_modal(frame, app, area);
    }

    if app.ui.theme_picker_open {
        render_theme_picker_modal(frame, app, area);
    }

    if app.ui.agent_picker_open {
        render_agent_picker_modal(frame, app, area);
    }

    if app.ui.screen == Screen::Sessions {
        crate::tui::render::sessions::render_sessions_popup(frame, app, area);
    }

    if app.ui.screen == Screen::Setup {
        crate::tui::render::setup::render_setup_modal(frame, app, area);
    }

    if let Some(crate::app::PendingTask::ConfirmFunction { .. }) = &app.proc.pending {
        render_confirm_modal(frame, app, area);
    }
}
