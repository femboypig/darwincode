use anyhow::Result;
use crossterm::event::{self, Event};
use std::time::Duration;

use crate::app::App;
use crate::tui::terminal::{start_terminal, stop_terminal};

pub fn run(app: App) -> Result<()> {
    let mut terminal = start_terminal()?;
    let (mut actor, tx) = crate::app::AppActor::new(app);

    // Spawn Crossterm input forwarding thread
    let tx_crossterm = tx.clone();
    std::thread::spawn(move || {
        loop {
            match event::poll(Duration::from_millis(100)) {
                Ok(true) => {
                    if let Ok(evt) = event::read() {
                        match evt {
                            Event::Key(key) => {
                                if key.kind != event::KeyEventKind::Release
                                    && tx_crossterm
                                        .send(crate::app::AppCommand::KeyEvent(key))
                                        .is_err()
                                {
                                    break;
                                }
                            }
                            Event::Mouse(mouse_event) => {
                                if tx_crossterm
                                    .send(crate::app::AppCommand::MouseEvent(mouse_event))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Event::Paste(text) => {
                                if tx_crossterm
                                    .send(crate::app::AppCommand::Paste(text))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Event::Resize(_, _) => {
                                if tx_crossterm.send(crate::app::AppCommand::Resize).is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });

    // Spawn tick generator task
    let tx_tick = tx.clone();
    crate::tui::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
            interval.tick().await;
            if tx_tick.send(crate::app::AppCommand::Tick).is_err() {
                break;
            }
        }
    });

    let result = crate::tui::async_runtime::block_on(async { actor.run(&mut terminal).await });

    stop_terminal(&mut terminal)?;

    let session_id = actor.app.chat.session_id.clone();
    println!("\nTo resume this session, run:");
    println!("  darwincode --session {}", session_id);
    println!("or continue the last session with:");
    println!("  darwincode --continue\n");

    result
}
