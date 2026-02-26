use clap::{Arg, Command};
use clap_complete::{generate, Shell};
use std::fs;
use std::io::BufWriter;

fn build_cli() -> Command {
    Command::new("chattor")
        .about("Private P2P chat over Tor")
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .action(clap::ArgAction::SetTrue)
                .help("Enable debug logging"),
        )
        .arg(
            Arg::new("config-dir")
                .short('c')
                .long("config-dir")
                .value_name("PATH")
                .help("Override config directory"),
        )
        .arg(
            Arg::new("data-dir")
                .long("data-dir")
                .value_name("PATH")
                .help("Override data directory"),
        )
        .subcommand(
            Command::new("tui")
                .about("Run interactive TUI (default if no subcommand)")
                .arg(
                    Arg::new("theme")
                        .short('t')
                        .long("theme")
                        .value_name("NAME")
                        .help("Theme preset (dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn)"),
                ),
        )
        .subcommand(Command::new("daemon").about("Run headless daemon"))
        .subcommand(Command::new("status").about("Show daemon status"))
        .subcommand(Command::new("identity").about("Show own identity"))
        .subcommand(
            Command::new("friends")
                .about("Friend management")
                .subcommand(Command::new("list").about("List friends"))
                .subcommand(
                    Command::new("add")
                        .about("Add a friend")
                        .arg(Arg::new("code").required(true)),
                )
                .subcommand(
                    Command::new("remove")
                        .about("Remove a friend")
                        .arg(Arg::new("onion").required(true)),
                )
                .subcommand(Command::new("requests").about("List pending requests"))
                .subcommand(
                    Command::new("accept")
                        .about("Accept a friend request")
                        .arg(Arg::new("id").required(true)),
                )
                .subcommand(
                    Command::new("reject")
                        .about("Reject a friend request")
                        .arg(Arg::new("id").required(true)),
                ),
        )
        .subcommand(
            Command::new("send")
                .about("Send a message")
                .arg(Arg::new("peer").required(true))
                .arg(Arg::new("message").required(true)),
        )
        .subcommand(
            Command::new("recv")
                .about("Receive unread messages")
                .arg(Arg::new("peer").long("peer").value_name("ONION")),
        )
        .subcommand(Command::new("listen").about("Stream incoming messages (blocking)"))
        .subcommand(
            Command::new("channels")
                .about("Channel management")
                .subcommand(Command::new("list").about("List channels"))
                .subcommand(
                    Command::new("publish")
                        .about("Publish to a channel")
                        .arg(Arg::new("channel_type").required(true))
                        .arg(Arg::new("message").required(true)),
                )
                .subcommand(
                    Command::new("subscribe")
                        .about("Subscribe to a channel")
                        .arg(Arg::new("onion").required(true)),
                )
                .subcommand(
                    Command::new("feed")
                        .about("Read channel feed")
                        .arg(Arg::new("channel").long("channel").value_name("ID")),
                ),
        )
        .subcommand(
            Command::new("ephemeral")
                .about("Set ephemeral message TTL")
                .arg(Arg::new("peer").required(true))
                .arg(Arg::new("ttl").required(true)),
        )
        .subcommand(
            Command::new("notifications")
                .about("Toggle notifications")
                .arg(Arg::new("state").required(true)),
        )
        .subcommand(Command::new("mcp").about("Start MCP server (stdio transport)"))
}

fn main() {
    let outdir = std::path::PathBuf::from("completions");
    fs::create_dir_all(&outdir).unwrap();

    let mut cmd = build_cli();

    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
        let filename = match shell {
            Shell::Bash => "chattor.bash",
            Shell::Zsh => "_chattor",
            Shell::Fish => "chattor.fish",
            _ => unreachable!(),
        };
        let path = outdir.join(filename);
        let mut file = BufWriter::new(fs::File::create(&path).unwrap());
        generate(shell, &mut cmd, "chattor", &mut file);
    }
}
