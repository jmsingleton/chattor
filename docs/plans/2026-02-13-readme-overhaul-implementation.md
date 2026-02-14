# README Overhaul Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the outdated README with a portfolio-quality document reflecting chattor's current state (Phases 1–5 complete), and delete stale companion docs.

**Architecture:** Single-file rewrite of `README.md` with 8 sections: hero, architecture, features, technical highlights, quick start, project status, project structure, footer. Also delete `TESTING.md` and `MIGRATION_FIX.md`.

**Tech Stack:** Markdown. No code changes — documentation only.

---

### Task 1: Delete stale documentation files

**Files:**
- Delete: `TESTING.md`
- Delete: `MIGRATION_FIX.md`

**Step 1: Verify files are stale**

Read `TESTING.md` and `MIGRATION_FIX.md`. Confirm they reference Phase 2 stubs and schema v2, which are no longer current (schema is v7, Signal crypto is real, Phases 3–5 are complete).

**Step 2: Delete the files**

```bash
git rm TESTING.md MIGRATION_FIX.md
```

**Step 3: Commit**

```bash
git commit -m "docs: remove stale TESTING.md and MIGRATION_FIX.md"
```

---

### Task 2: Write README hero + intro section

**Files:**
- Modify: `README.md` (replace entire contents)

**Step 1: Write the hero and intro**

Replace the entire `README.md` with the opening sections:

```markdown
# chattor

**Peer-to-peer encrypted chat over Tor, right in your terminal.**

A TUI chat application where each user runs their own Tor hidden service.
Messages are end-to-end encrypted with Signal Protocol (Double Ratchet),
stored in an encrypted local database, and routed entirely through Tor.
No central servers. No accounts. No metadata leakage.

Built in Rust with ratatui.
```

**Step 2: Verify rendering**

Visually check that the markdown renders correctly (heading, bold tagline, paragraph, closing line).

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: rewrite README hero and intro"
```

---

### Task 3: Add architecture overview section

**Files:**
- Modify: `README.md` (append after intro)

**Step 1: Add architecture section**

Append after the intro:

```markdown
## How It Works

```
You ──→ TUI (ratatui) ──→ Signal Protocol (E2E) ──→ Tor Hidden Service ──→ Peer
                                                          ↑
                                                   .onion address = your identity
```

Each chattor instance is both a client and a server:

- **Your identity** is an Ed25519 keypair. Your .onion address is derived from it.
- **Sending a message:** encrypt with Signal Protocol → route through Tor → deliver to peer's hidden service
- **Receiving a message:** your hidden service accepts connections → decrypt → store locally
- **Offline delivery:** messages queue locally and retry automatically when the peer comes back online
```

**Step 2: Verify accuracy**

Cross-reference with `src/app.rs` (App struct owns identity, tor_client, hidden_service, message_queue) and `src/crypto/signal.rs` (encrypt/decrypt flow). Confirm the architecture description matches reality.

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add architecture overview to README"
```

---

### Task 4: Add features section

**Files:**
- Modify: `README.md` (append after architecture)

**Step 1: Add features**

Append the four feature groups. Key details to get right:
- Friend codes are 32-word mnemonics (8 groups of 4 words) — see `src/protocol/friend_code.rs`
- 7 theme presets — see `src/cli.rs` line 15 and `src/ui/theme.rs`
- 12 message types — see `src/protocol/message.rs`
- Signal Protocol uses X3DH + Double Ratchet — see `src/crypto/signal.rs`

```markdown
## Features

**Encryption & Identity**
- Signal Protocol (Double Ratchet) for end-to-end encryption with forward secrecy
- X3DH key exchange for establishing encrypted sessions
- Vanity .onion address mining — choose a custom prefix for your identity
- Ed25519 identity keypair with deterministic .onion derivation
- SQLCipher encrypted local database

**Networking**
- Pure peer-to-peer over Tor — each user hosts a v3 hidden service
- Offline message queue with automatic retry (configurable attempts and intervals)
- Friend codes — 32-word mnemonic encoding of your public key
- Connection pooling for efficient peer communication

**Broadcast Channels**
- Create public or friends-only broadcast channels
- Ed25519-signed posts with pull-based sync
- Read receipts and 100-post retention
- Auto-subscribe to friends' channels

**Terminal UI**
- 7 built-in themes: dark, light, cyberpunk, minimal, and 3 rosé pine variants
- Custom theming via TOML config (`~/.config/chattor/theme.toml`)
- Animated bootstrap screen during Tor connection
- Full-screen vanity mining UI with live stats and ETA
- Friends sidebar, conversation view, channel feed
- Clipboard integration (wl-copy, xclip, xsel, pbcopy)
```

**Step 2: Spot-check claims**

Verify: theme count in `src/ui/theme.rs`, friend code format in `src/protocol/friend_code.rs`, channel features in `src/db/queries.rs` and `src/ui/channel_feed.rs`.

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add features section to README"
```

---

### Task 5: Add technical highlights section

**Files:**
- Modify: `README.md` (append after features)

**Step 1: Add highlights**

```markdown
## Technical Highlights

A few pieces that were particularly interesting to build:

- **Vanity mining** — parallel rayon workers brute-force Ed25519 keypairs to match a
  user-chosen .onion prefix, with live ETA estimation and ASCII art mining UI
- **Signal Protocol from scratch** — X3DH handshake and Double Ratchet using
  `x25519-dalek` and `chacha20poly1305`, not a binding to libsignal-c
- **Database encryption** — SQLCipher (bundled via rusqlite) encrypts the entire
  database at rest, with full-text search via FTS5 virtual tables and auto-sync triggers
- **Theme engine** — a `Theme` struct threads consistent colors through every UI
  component, with 7 presets and TOML override support for custom color schemes
```

**Step 2: Verify references**

- Vanity mining: `src/crypto/vanity.rs` (rayon workers), `src/ui/mining.rs` (UI)
- Signal: `src/crypto/signal.rs` (X3DH, Double Ratchet, ChaCha20-Poly1305)
- SQLCipher: `Cargo.toml` line 23 (`bundled-sqlcipher`), `src/db/schema.rs` (FTS5)
- Theme: `src/ui/theme.rs`

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add technical highlights to README"
```

---

### Task 6: Add quick start and CLI section

**Files:**
- Modify: `README.md` (append after highlights)

**Step 1: Add quick start**

```markdown
## Quick Start

```bash
# Build
cargo build --release

# Run
cargo run

# Run with a theme
cargo run -- --theme cyberpunk

# Run tests
cargo test
```

### CLI Options

| Flag | Description |
|------|-------------|
| `--debug` / `-d` | Enable debug logging |
| `--theme <name>` / `-t` | Theme preset: `dark`, `light`, `cyberpunk`, `minimal`, `rose-pine`, `rose-pine-moon`, `rose-pine-dawn` |
| `--config-dir <path>` / `-c` | Custom config/data directory |
```

**Step 2: Verify CLI flags**

Cross-reference with `src/cli.rs`. Confirm short flags (`-d`, `-t`, `-c`) exist via clap derive macros.

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add quick start and CLI section to README"
```

---

### Task 7: Add project status and structure sections

**Files:**
- Modify: `README.md` (append after quick start)

**Step 1: Add project status table**

```markdown
## Project Status

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Core Foundation | Done |
| 2 | Tor + Messaging Foundation | Done |
| 3 | Broadcast Channels | Done |
| 4 | Polish & Theming | Done |
| 5 | Crypto & Identity Hardening | Done |
| 6 | Hardening & Packaging | In Progress |

> **Note:** Tor integration is currently stubbed — the encryption, database, UI, protocol,
> and channel systems are fully implemented, but real Tor network connections are not yet
> wired in. Messages encrypt and decrypt with real Signal Protocol cryptography, but
> delivery requires completing the Tor transport layer.
```

**Step 2: Add project structure tree**

```markdown
## Project Structure

```
src/
├── app.rs              # Application state and initialization
├── cli.rs              # CLI argument parsing (clap)
├── error.rs            # Error types (thiserror)
├── main.rs             # Entry point, event loop, key handling
├── crypto/
│   ├── identity.rs     # Ed25519 keypair management
│   ├── signal.rs       # Signal Protocol (X3DH + Double Ratchet)
│   └── vanity.rs       # Vanity .onion mining with rayon
├── db/
│   ├── connection.rs   # SQLCipher database + migrations (v2→v7)
│   ├── queries.rs      # All database queries
│   └── schema.rs       # Schema definitions (v7)
├── net/
│   ├── queue.rs            # Offline message queue
│   ├── queue_processor.rs  # Background queue processing
│   └── pool.rs             # Connection pooling
├── protocol/
│   ├── friend_code.rs      # 32-word mnemonic friend codes
│   ├── friend_request.rs   # Friend request protocol
│   └── message.rs          # Wire protocol (12 message types)
├── tor/
│   ├── client.rs           # Tor client wrapper (arti)
│   ├── hidden_service.rs   # Hidden service hosting
│   ├── connection.rs       # Peer connections over Tor
│   └── address.rs          # .onion address utilities
└── ui/
    ├── theme.rs        # Theme engine (7 presets + TOML config)
    ├── channel_feed.rs # Broadcast channel UI
    ├── mining.rs       # Vanity mining UI screens
    └── ...             # Sidebar, modals, header, footer
```
```

**Step 3: Verify structure**

Run `ls -R src/` and confirm the tree matches actual files. Check `src/db/schema.rs` for current schema version number.

**Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add project status and structure to README"
```

---

### Task 8: Add footer and final review

**Files:**
- Modify: `README.md` (append at end)

**Step 1: Add requirements and license**

```markdown
## Requirements

- Rust 1.70+ (2021 edition)
- Linux or macOS (no Windows support)
- SQLCipher (bundled automatically via rusqlite)

## License

MIT OR Apache-2.0
```

Verify license in `Cargo.toml` line 7.

**Step 2: Full review pass**

Read the complete `README.md` from top to bottom. Check for:
- Consistent formatting (headings, code blocks, tables)
- No references to old format (`word-NNNN-word-NNNN`)
- No claims about Phase 2b being "in progress"
- Accurate test count, theme count, message type count
- All code blocks properly fenced

**Step 3: Run tests to confirm nothing is broken**

```bash
cargo test
```

Expected: 209+ tests pass (documentation changes shouldn't affect tests, but verify).

**Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add requirements, license, and final polish to README"
```
