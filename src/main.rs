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

fn main() -> Result<()> {
    let _cli = Cli::parse();
    println!("torrent-chat v0.1.0");
    Ok(())
}
