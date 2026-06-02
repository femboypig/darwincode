use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

pub(crate) fn logo_lines() -> Vec<Line<'static>> {
    let lines = vec![
        "   ▄▄                                              ▄▄       ",
        "   ██                     ▀▀                       ██       ",
        "▄████  ▀▀█▄ ████▄ ██   ██ ██  ████▄ ▄████ ▄███▄ ▄████ ▄█▀█▄ ",
        "██ ██ ▄█▀██ ██ ▀▀ ██ █ ██ ██  ██ ██ ██    ██ ██ ██ ██ ██▄█▀ ",
        "▀████ ▀█▄██ ██     ██▀██  ██▄ ██ ██ ▀████ ▀███▀ ▀████ ▀█▄▄▄ ",
    ];

    lines
        .into_iter()
        .map(|line| Line::from(Span::styled(line, Style::default())))
        .collect()
}

pub(crate) fn logo_lines_for_area(width: u16, max_height: u16) -> Vec<Line<'static>> {
    let lines = logo_lines();
    if logo_fits(&lines, width, max_height) {
        lines
    } else {
        Vec::new()
    }
}

pub(crate) fn logo_fits(lines: &[Line<'_>], width: u16, max_height: u16) -> bool {
    !lines.is_empty()
        && lines.len() <= max_height as usize
        && lines.iter().all(|line| line.width() <= width as usize)
}

pub(crate) fn welcome_lines(area: Rect) -> Vec<Line<'static>> {
    let max_logo_height = area.height.saturating_sub(2).max(1);
    logo_lines_for_area(area.width, max_logo_height)
}
