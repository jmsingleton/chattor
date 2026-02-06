mod error;
mod app;
mod cli;
mod config;
mod crypto;
mod db;
mod tor;
mod protocol;
mod net;
mod ui;

use clap::Parser;
use cli::Cli;
use error::Result;
use ui::AppUI;

fn main() -> Result<()> {
    let _cli = Cli::parse();

    let mut ui = AppUI::new();
    ui.run()?;

    Ok(())
}
