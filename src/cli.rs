use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "chattor")]
#[command(about = "Privacy-first TUI chat application over Tor", long_about = None)]
pub struct Cli {
    /// Enable debug logging
    #[arg(short, long)]
    pub debug: bool,

    /// Config directory path
    #[arg(short, long)]
    pub config_dir: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let cli = Cli::parse_from(["chattor", "--debug"]);
        assert!(cli.debug);
    }
}
