# User Documentation — Design

## Overview

Add terminal-accessible user documentation: a hand-written man page (`man chattor`) and enhanced `--help` output. Comprehensive coverage of keybindings, concepts, getting started, CLI usage, and file locations. Also clean up stale progress docs.

## Audience & Tone

- **Audience:** Users and developers running chattor from the terminal
- **Tone:** Same as README — cheeky and smart, yet professional. Man page sections are terse per convention; descriptions are clear and direct.

## Approach

Hand-written ROFF man page + clap `after_long_help`. No new dependencies. No build-time generation. The man page is a static file in the repo.

## Deliverable 1: Man Page (`man/chattor.1`)

Hand-crafted ROFF file with these sections:

- **NAME** — chattor - peer-to-peer encrypted chat over Tor
- **SYNOPSIS** — chattor [OPTIONS]
- **DESCRIPTION** — What chattor is, how it works (P2P hidden services, Signal Protocol, encrypted DB), what happens on first run (vanity mining)
- **OPTIONS** — All 3 flags with descriptions (--debug, --theme, --config-dir)
- **GETTING STARTED** — First run flow: mining screen → identity → add friend → send message
- **KEYBINDINGS** — Table organized by context: navigation, chat, modals, identity, mining
- **FRIEND CODES** — What they are (32-word mnemonic = public key), how to share, example format
- **BROADCAST CHANNELS** — Public vs friends-only, posting, subscribing, sync behavior
- **THEMES** — 7 presets listed, TOML override path
- **FILES** — Config/data paths for Linux and macOS
- **EXAMPLES** — Common invocations
- **SEE ALSO** — tor(1), arti(1)

## Deliverable 2: Enhanced `--help` (`src/cli.rs`)

Modify clap Command to add:

- `long_about`: Multi-line description replacing the current one-liner
- `after_long_help`: Keybindings cheat sheet, first-run note, file paths, pointer to man page

The short help (`-h`) stays concise. The long help (`--help`) shows the full version.

## Deliverable 3: Stale Doc Cleanup

Delete outdated progress docs that are superseded by CLAUDE.md and README:

- `docs/Phase2-Progress.md`
- `docs/Phase2b-Progress.md`
- `docs/phase1-progress.md`
- `docs/Testing-Phase2b.md`

## What's NOT in Scope

- In-app help modal (could be a future addition)
- HTML or web documentation
- Shell completions (potential future work with clap_complete)
- Packaging/installation of the man page (that's Phase 6 packaging work)
