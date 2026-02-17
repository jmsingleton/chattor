use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "chattor")]
#[command(
    about = "Privacy-first TUI chat application over Tor",
    long_about = "Peer-to-peer encrypted chat over Tor, right in your terminal.\n\n\
        Each user runs their own Tor hidden service. Messages are end-to-end\n\
        encrypted with Signal Protocol (Double Ratchet), stored in a local\n\
        encrypted database, and routed through Tor. No servers, no accounts,\n\
        no metadata leakage.",
    after_long_help = "KEYBINDINGS\n  Tab / ↑↓     Navigate friends list\n  Enter        Select friend / send message\n  a            Add friend        i   Identity\n  s            Subscribe         p   Public channel\n  f            Friend requests   q   Quit\n  Esc          Back / cancel\n\nFIRST RUN\n  On first launch an Ed25519 identity is generated automatically.\n  Your .onion address is assigned when Tor connects.\n\nFILES\n  ~/.config/chattor/theme.toml    Theme overrides\n  ~/.local/share/chattor/         Database & identity (Linux)\n  ~/Library/Application Support/chattor/  (macOS)\n\nSee chattor(1) man page for the full manual."
)]
pub struct Cli {
    /// Enable debug logging
    #[arg(short, long)]
    pub debug: bool,

    /// Config directory path (overrides default ~/.config/chattor/)
    #[arg(short, long)]
    pub config_dir: Option<String>,

    /// Data directory path (overrides default ~/.local/share/chattor/)
    #[arg(long)]
    pub data_dir: Option<String>,

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
