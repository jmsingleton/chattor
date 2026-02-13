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

    /// Theme preset (dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn)
    #[arg(short, long)]
    pub theme: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let cli = Cli::parse_from(["chattor", "--debug"]);
        assert!(cli.debug);
    }

    #[test]
    fn test_cli_theme_flag() {
        let cli = Cli::parse_from(["chattor", "--theme", "cyberpunk"]);
        assert_eq!(cli.theme.as_deref(), Some("cyberpunk"));
    }

    #[test]
    fn test_cli_no_theme() {
        let cli = Cli::parse_from(["chattor"]);
        assert!(cli.theme.is_none());
    }
}
