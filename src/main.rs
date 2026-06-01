mod app;
mod config;
mod crypto;
mod gemini;
mod tui;

use anyhow::Result;

use crate::app::App;
use crate::config::StoredConfig;

fn main() -> Result<()> {
    let config = StoredConfig::load()?;
    let app = App::new(config);
    tui::run(app)
}
