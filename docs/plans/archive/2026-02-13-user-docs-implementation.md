# User Documentation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add terminal-accessible user documentation — a hand-written man page and enhanced `--help` output — plus clean up stale progress docs.

**Architecture:** Hand-crafted ROFF man page at `man/chattor.1` with full user reference (keybindings, concepts, getting started). Enhanced `--help` via clap's `long_about` and `after_long_help` in `src/cli.rs`. No new dependencies.

**Tech Stack:** ROFF (man page format), clap derive macros (existing dependency)

---

### Task 1: Delete stale progress docs

**Files:**
- Delete: `docs/Phase2-Progress.md`
- Delete: `docs/Phase2b-Progress.md`
- Delete: `docs/phase1-progress.md`
- Delete: `docs/Testing-Phase2b.md`

**Step 1: Delete the files**

```bash
git rm docs/Phase2-Progress.md docs/Phase2b-Progress.md docs/phase1-progress.md docs/Testing-Phase2b.md
```

These are historical progress tracking docs from Phases 1-2b. The project status is now tracked in CLAUDE.md and README.md. They reference outdated schema versions, stub behavior, and test counts.

**Step 2: Commit**

```bash
git commit -m "docs: remove stale phase progress docs — superseded by CLAUDE.md and README"
```

---

### Task 2: Write the man page

**Files:**
- Create: `man/chattor.1`

**Step 1: Create the `man/` directory**

```bash
mkdir -p man
```

**Step 2: Write the ROFF man page**

Create `man/chattor.1` with the following content. This is the complete man page — write it exactly as specified.

ROFF crash course for the implementer:
- `.TH` = title header (name, section, date)
- `.SH` = section header
- `.SS` = subsection header
- `.B` = bold, `.I` = italic, `.BI` = bold-italic alternating
- `.TP` = tagged paragraph (for option lists — tag on next line, body indented after)
- `.br` = line break
- `.PP` = new paragraph
- `\fB...\fR` = inline bold, `\fI...\fR` = inline italic

The man page should contain these sections in this order:

**NAME section:**
```
chattor \- peer-to-peer encrypted chat over Tor
```

**SYNOPSIS section:**
```
.B chattor
.RI [ OPTIONS ]
```

**DESCRIPTION section** (3 paragraphs):
1. What chattor is: a TUI chat application where each user runs a Tor hidden service. Messages are E2E encrypted with Signal Protocol (Double Ratchet). No central servers, no accounts, no metadata leakage.
2. How identity works: your Ed25519 keypair IS your identity. The public key derives your v3 .onion address. On first launch, chattor offers vanity mining to choose a custom .onion prefix.
3. How storage works: all data stored locally in a SQLCipher-encrypted database. Messages are searchable via full-text search. Nothing is stored on remote servers.

**OPTIONS section** — use `.TP` for each:
- `.BR \-d ", " \-\-debug` — Enable debug logging. Log output goes to stderr.
- `.BR \-t ", " \-\-theme " " \fIname\fR` — Set theme preset. Available: dark (default), light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn.
- `.BR \-c ", " \-\-config\-dir " " \fIpath\fR` — Use a custom config/data directory instead of the platform default.
- `.BR \-h ", " \-\-help` — Print help and exit.

**GETTING STARTED section** (numbered steps):
1. Build and run: `cargo build --release && cargo run`
2. On first launch, the mining screen appears. Type a prefix for your .onion address (e.g., "chat") and press Enter to start mining, or press Esc to skip and get a random address.
3. Once mining completes (or is skipped), the main TUI loads. Your identity is shown in the identity modal (press `i`).
4. Share your friend code or .onion address with someone. They add you with `a` and paste your address.
5. When a friend request arrives, press `f` to view pending requests, then `A` to accept.
6. Select a friend with Tab/arrows and Enter, then type a message and press Enter to send.

**KEYBINDINGS section** — use subsections (`.SS`) per context:

*Navigation (sidebar focused):*
- Tab, Up/Down — move between friends
- Enter — select friend / open conversation
- a — add a friend
- i — view identity
- s — subscribe to a channel
- p — open your public channel
- f — view friend requests
- q — quit

*Chat (input focused):*
- Enter — send message
- Esc — return to navigation

*Identity Modal:*
- o — copy .onion address to clipboard
- c — copy friend code to clipboard
- i or Esc — close

*Friend Requests:*
- Up/Down — navigate request list
- Enter — view request details
- A — accept request
- R — reject request
- Esc — go back

*Ephemeral Settings:*
- Up/Down — select duration
- Enter — confirm
- Esc — cancel

*Channel View (own channel):*
- Enter — compose and post
- Esc — return to sidebar

*Channel View (subscribed):*
- Esc — return to sidebar

*Mining Screen:*
- Enter — start mining with entered prefix
- Esc — skip, use random address

**FRIEND CODES section:**
Explain that friend codes are 32-word mnemonics derived from your Ed25519 public key. Each byte of the 32-byte key maps to one word from a 256-word dictionary. Displayed as 8 groups of 4 words separated by dashes (groups separated by spaces). Example format: `ace-act-add-age ago-aid-aim-air ...` (8 groups total). Friend codes and .onion addresses are interchangeable — either can be pasted when adding a friend.

**BROADCAST CHANNELS section:**
Each user automatically has two channels: Public and Friends Only. Public channels are visible to anyone who knows your address. Friends Only channels are limited to accepted friends. Posts are Ed25519-signed by the publisher. Subscribers pull new posts periodically (every 5 minutes) via sync requests. Publishers see read receipt counts per post. Channels retain the 100 most recent posts.

**THEMES section:**
List all 7 presets with one-line descriptions:
- dark — muted grays and blue accents (default)
- light — bright background with dark text
- cyberpunk — neon greens and magentas on dark background
- minimal — monochrome, low contrast
- rose-pine — soft pink and gold on dark background
- rose-pine-moon — rose pine variant with cooler tones
- rose-pine-dawn — light rose pine variant

Note: custom colors via `~/.config/chattor/theme.toml`. Any color field from the Theme struct can be overridden with hex values.

**FILES section** — use `.TP` for each path:
- `~/.config/chattor/theme.toml` — Theme color overrides (Linux)
- `~/.local/share/chattor/messages.db` — Encrypted database (Linux)
- `~/Library/Application Support/chattor/` — Config and data directory (macOS)

**EXAMPLES section:**
```
.TP
.B chattor
Start chattor with default settings.
.TP
.B chattor \-\-theme cyberpunk
Start with the cyberpunk color theme.
.TP
.B chattor \-c /tmp/alice
Run an isolated instance (useful for testing with two users).
```

**SEE ALSO section:**
```
.BR tor (1),
.BR arti (1)
```

**Step 3: Test the man page renders correctly**

```bash
man ./man/chattor.1
```

Scroll through and verify: all sections render, bold/italic formatting works, tagged paragraphs are indented, no raw ROFF macros visible.

**Step 4: Commit**

```bash
git add man/chattor.1
git commit -m "docs: add hand-written man page — full user reference"
```

---

### Task 3: Enhance `--help` output

**Files:**
- Modify: `src/cli.rs`

**Step 1: Read the current cli.rs**

Read `/home/john/chattor/chattor/src/cli.rs` to understand the current clap configuration.

Current state: minimal — `about` is one line, no `long_about`, no `after_help`.

**Step 2: Add `long_about` and `after_long_help`**

Modify the `#[command]` attributes on the `Cli` struct:

- Replace the `about` with a short version for `-h`
- Add `long_about` with a multi-line description (what chattor is, P2P model, encryption)
- Add `after_long_help` with: keybindings cheat sheet, first-run note, file paths, pointer to man page

The `long_about` should be:
```
Peer-to-peer encrypted chat over Tor, right in your terminal.

Each user runs their own Tor hidden service. Messages are end-to-end
encrypted with Signal Protocol (Double Ratchet), stored in a local
encrypted database, and routed through Tor. No servers, no accounts,
no metadata leakage.
```

The `after_long_help` should be:
```
KEYBINDINGS
  Tab / ↑↓     Navigate friends list
  Enter        Select friend / send message
  a            Add friend        i   Identity
  s            Subscribe         p   Public channel
  f            Friend requests   q   Quit
  Esc          Back / cancel

FIRST RUN
  On first launch you'll choose a vanity .onion prefix (or press Esc to skip).
  Your identity is generated and saved locally.

FILES
  ~/.config/chattor/theme.toml    Theme overrides
  ~/.local/share/chattor/         Database & identity (Linux)
  ~/Library/Application Support/chattor/  (macOS)

See man chattor(1) for the full manual.
```

**Step 3: Verify the output**

```bash
cargo run -- --help
```

Expected: the enhanced output with description, options, keybindings, first-run note, files, and man page pointer.

Also verify short help is still concise:
```bash
cargo run -- -h
```

Expected: short `about` text, options list, no after_help content.

Note: clap's behavior is that `-h` shows `about` + options, while `--help` shows `long_about` + options + `after_long_help`. This is the default with `#[command(about = "...", long_about = "...")]`.

**Step 4: Run existing tests**

```bash
cargo test cli
```

Expected: existing CLI tests still pass. The tests use `Cli::parse_from` which doesn't exercise help text.

**Step 5: Commit**

```bash
git add src/cli.rs
git commit -m "docs: enhance --help with keybindings, first-run guide, and file paths"
```

---

### Task 4: Final verification

**Step 1: Verify man page**

```bash
man ./man/chattor.1
```

Check: all sections render, formatting is correct, content is accurate.

**Step 2: Verify --help**

```bash
cargo run -- --help
```

Check: description, options, keybindings, files, man page pointer all present.

```bash
cargo run -- -h
```

Check: short help is still concise.

**Step 3: Run full test suite**

```bash
cargo test
```

Expected: all tests pass (documentation changes shouldn't affect tests).

**Step 4: Commit if any final fixes needed**

```bash
git add -A
git commit -m "docs: final polish on user documentation"
```
