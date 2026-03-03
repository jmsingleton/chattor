use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "chattor", about = "Private P2P chat over Tor")]
#[command(
    long_about = "Peer-to-peer encrypted chat over Tor, right in your terminal.\n\n\
        Each user runs their own Tor hidden service. Messages are end-to-end\n\
        encrypted with Signal Protocol (Double Ratchet), stored in a local\n\
        encrypted database, and routed through Tor. No servers, no accounts,\n\
        no metadata leakage.",
    after_long_help = "KEYBINDINGS (TUI mode)\n  Tab / \u{2191}\u{2193}     Navigate friends list\n  Enter        Select friend / send message\n  a            Add friend        i   Identity\n  s            Subscribe         p   Public channel\n  f            Friend requests   q   Quit\n  Esc          Back / cancel\n\nFIRST RUN\n  On first launch an Ed25519 identity is generated automatically.\n  Your .onion address is assigned when Tor connects.\n\nFILES\n  ~/.config/chattor/theme.toml    Theme overrides\n  ~/.local/share/chattor/         Database & identity (Linux)\n  ~/Library/Application Support/chattor/  (macOS)\n\nSee chattor(1) man page for the full manual."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Enable debug logging
    #[arg(short, long)]
    pub debug: bool,

    /// Override config directory
    #[arg(short, long)]
    pub config_dir: Option<String>,

    /// Override data directory
    #[arg(long)]
    pub data_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run interactive TUI (default if no subcommand)
    Tui {
        /// Theme preset (dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn)
        #[arg(short, long)]
        theme: Option<String>,
    },

    /// Run headless daemon
    Daemon {
        /// Read passphrase from file descriptor (for automation)
        #[arg(long)]
        passphrase_fd: Option<i32>,
    },

    /// Show daemon status
    Status,

    /// Show own identity (friend code + .onion address)
    Identity,

    /// Friend management
    Friends {
        #[command(subcommand)]
        action: FriendsAction,
    },

    /// Send a message
    Send {
        /// Peer .onion address or friend code
        peer: String,
        /// Message text
        message: String,
    },

    /// Receive unread messages
    Recv {
        /// Filter by peer
        #[arg(long)]
        peer: Option<String>,
    },

    /// Stream incoming messages (blocking)
    Listen,

    /// Channel management
    Channels {
        #[command(subcommand)]
        action: ChannelsAction,
    },

    /// Set ephemeral message TTL
    Ephemeral {
        /// Peer .onion or friend code
        peer: String,
        /// TTL in seconds (0 to disable)
        ttl: i64,
    },

    /// Toggle notifications
    Notifications {
        /// on or off
        state: String,
    },

    /// Start MCP server (stdio transport)
    Mcp,
}

#[derive(Subcommand, Debug)]
pub enum FriendsAction {
    /// List friends
    List,
    /// Add a friend
    Add { code: String },
    /// Remove a friend
    Remove { onion: String },
    /// List pending requests
    Requests,
    /// Accept a friend request
    Accept { id: i64 },
    /// Reject a friend request
    Reject { id: i64 },
}

#[derive(Subcommand, Debug)]
pub enum ChannelsAction {
    /// List channels
    List,
    /// Publish to a channel
    Publish {
        /// "public" or "friends"
        channel_type: String,
        /// Post content
        message: String,
    },
    /// Subscribe to a channel
    Subscribe { onion: String },
    /// Read channel feed
    Feed {
        #[arg(long)]
        channel: Option<i64>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_default_no_subcommand() {
        let cli = Cli::parse_from(["chattor"]);
        assert!(!cli.debug);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_debug_flag() {
        let cli = Cli::parse_from(["chattor", "--debug"]);
        assert!(cli.debug);
    }

    #[test]
    fn test_cli_tui_subcommand_with_theme() {
        let cli = Cli::parse_from(["chattor", "tui", "--theme", "cyberpunk"]);
        match cli.command {
            Some(Command::Tui { theme }) => {
                assert_eq!(theme.as_deref(), Some("cyberpunk"));
            }
            _ => panic!("Expected Tui subcommand"),
        }
    }

    #[test]
    fn test_cli_daemon_subcommand() {
        let cli = Cli::parse_from(["chattor", "daemon"]);
        assert!(matches!(cli.command, Some(Command::Daemon { .. })));
    }

    #[test]
    fn test_cli_status_subcommand() {
        let cli = Cli::parse_from(["chattor", "status"]);
        assert!(matches!(cli.command, Some(Command::Status)));
    }

    #[test]
    fn test_cli_identity_subcommand() {
        let cli = Cli::parse_from(["chattor", "identity"]);
        assert!(matches!(cli.command, Some(Command::Identity)));
    }

    #[test]
    fn test_cli_friends_list() {
        let cli = Cli::parse_from(["chattor", "friends", "list"]);
        match cli.command {
            Some(Command::Friends {
                action: FriendsAction::List,
            }) => {}
            _ => panic!("Expected Friends List subcommand"),
        }
    }

    #[test]
    fn test_cli_friends_add() {
        let cli = Cli::parse_from(["chattor", "friends", "add", "some-friend-code"]);
        match cli.command {
            Some(Command::Friends {
                action: FriendsAction::Add { code },
            }) => {
                assert_eq!(code, "some-friend-code");
            }
            _ => panic!("Expected Friends Add subcommand"),
        }
    }

    #[test]
    fn test_cli_send_message() {
        let cli = Cli::parse_from(["chattor", "send", "peer.onion", "hello world"]);
        match cli.command {
            Some(Command::Send { peer, message }) => {
                assert_eq!(peer, "peer.onion");
                assert_eq!(message, "hello world");
            }
            _ => panic!("Expected Send subcommand"),
        }
    }

    #[test]
    fn test_cli_recv_with_peer_filter() {
        let cli = Cli::parse_from(["chattor", "recv", "--peer", "abc.onion"]);
        match cli.command {
            Some(Command::Recv { peer }) => {
                assert_eq!(peer.as_deref(), Some("abc.onion"));
            }
            _ => panic!("Expected Recv subcommand"),
        }
    }

    #[test]
    fn test_cli_listen() {
        let cli = Cli::parse_from(["chattor", "listen"]);
        assert!(matches!(cli.command, Some(Command::Listen)));
    }

    #[test]
    fn test_cli_channels_list() {
        let cli = Cli::parse_from(["chattor", "channels", "list"]);
        match cli.command {
            Some(Command::Channels {
                action: ChannelsAction::List,
            }) => {}
            _ => panic!("Expected Channels List subcommand"),
        }
    }

    #[test]
    fn test_cli_channels_publish() {
        let cli = Cli::parse_from(["chattor", "channels", "publish", "public", "Hello!"]);
        match cli.command {
            Some(Command::Channels {
                action:
                    ChannelsAction::Publish {
                        channel_type,
                        message,
                    },
            }) => {
                assert_eq!(channel_type, "public");
                assert_eq!(message, "Hello!");
            }
            _ => panic!("Expected Channels Publish subcommand"),
        }
    }

    #[test]
    fn test_cli_ephemeral() {
        let cli = Cli::parse_from(["chattor", "ephemeral", "peer.onion", "3600"]);
        match cli.command {
            Some(Command::Ephemeral { peer, ttl }) => {
                assert_eq!(peer, "peer.onion");
                assert_eq!(ttl, 3600);
            }
            _ => panic!("Expected Ephemeral subcommand"),
        }
    }

    #[test]
    fn test_cli_notifications() {
        let cli = Cli::parse_from(["chattor", "notifications", "on"]);
        match cli.command {
            Some(Command::Notifications { state }) => {
                assert_eq!(state, "on");
            }
            _ => panic!("Expected Notifications subcommand"),
        }
    }

    #[test]
    fn test_cli_mcp() {
        let cli = Cli::parse_from(["chattor", "mcp"]);
        assert!(matches!(cli.command, Some(Command::Mcp)));
    }

    #[test]
    fn test_cli_config_dir_override() {
        let cli = Cli::parse_from(["chattor", "--config-dir", "/tmp/chattor-cfg"]);
        assert_eq!(cli.config_dir.as_deref(), Some("/tmp/chattor-cfg"));
    }

    #[test]
    fn test_cli_data_dir_override() {
        let cli = Cli::parse_from(["chattor", "--data-dir", "/tmp/chattor-data"]);
        assert_eq!(cli.data_dir.as_deref(), Some("/tmp/chattor-data"));
    }
}
