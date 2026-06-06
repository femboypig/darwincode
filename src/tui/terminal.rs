use anyhow::Result;
use crossterm::event;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub fn start_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableFocusChange,
        event::EnableBracketedPaste,
        event::EnableMouseCapture
    )?;
    Terminal::new(CrosstermBackend::new(stdout)).map_err(Into::into)
}

pub fn stop_terminal(terminal: &mut Tui) -> Result<()> {
    let _ = execute!(
        io::stdout(),
        event::DisableFocusChange,
        event::DisableBracketedPaste,
        event::DisableMouseCapture
    );
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
