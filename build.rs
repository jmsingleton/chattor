use clap::{Arg, Command};
use clap_complete::{generate, Shell};
use std::fs;
use std::io::BufWriter;

fn build_cli() -> Command {
    Command::new("chattor")
        .about("Privacy-first TUI chat application over Tor")
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
                .help("Config directory path"),
        )
        .arg(
            Arg::new("theme")
                .short('t')
                .long("theme")
                .value_name("NAME")
                .help("Theme preset (dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn)"),
        )
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
