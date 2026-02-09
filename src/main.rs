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
use app::App;
use ui::AppUI;

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();

    // Initialize application
    let mut app = App::new()?;

    // Initialize Tor in background
    // TODO: Make App Clone or use Arc for proper async sharing
    tokio::spawn(async move {
        println!("Bootstrapping Tor connection...");
        match app.init_tor().await {
            Ok(_) => println!("Tor connected!"),
            Err(e) => eprintln!("Tor initialization failed: {}", e),
        }
    });

    // Run TUI
    let mut ui = AppUI::new();
    ui.run()?;

    Ok(())
}
