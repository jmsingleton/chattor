# README Overhaul — Design

## Overview

Replace the outdated README (stuck at Phase 2b) with a portfolio-quality document that reflects the current state of chattor through Phase 5. Also delete stale companion docs (`TESTING.md`, `MIGRATION_FIX.md`).

## Audience & Tone

- **Audience:** Portfolio visitors — people evaluating the technical depth of the project
- **Tone:** Cheeky and smart, yet professional. Let the technical facts speak for themselves. Confident, not preachy.

## Approach

Feature-Forward Showcase: lead with what makes chattor interesting, then back it up with technical substance.

## Structure

### 1. Hero + Intro
- Project name
- One-line tagline: "Peer-to-peer encrypted chat over Tor, right in your terminal."
- Single paragraph explaining the core architecture: each user = Tor hidden service, Signal Protocol E2E, encrypted local DB, Tor routing, no central servers
- "Built in Rust with ratatui."

### 2. Architecture Overview
- Simple ASCII diagram: `You → TUI → Signal Protocol → Tor → Peer`
- 4 bullet points explaining the P2P symmetry: identity, sending, receiving, offline delivery

### 3. Features
Grouped into 4 categories:
- **Encryption & Identity:** Signal Protocol, X3DH, vanity mining, Ed25519 identity, SQLCipher
- **Networking:** P2P over Tor, offline queue, friend codes (32-word mnemonic, 8 groups of 4 words), connection pooling
- **Broadcast Channels:** Public/friends-only channels, signed posts, pull-based sync, read receipts, auto-subscribe
- **Terminal UI:** 7 themes, TOML config, animated bootstrap, mining UI, sidebar/conversation/channel views, clipboard

### 4. Technical Highlights
Portfolio section — "things that were fun to build":
- Vanity mining with rayon parallel workers + live ETA
- Signal Protocol from scratch (X3DH + Double Ratchet with x25519-dalek + chacha20poly1305)
- SQLCipher encrypted DB with FTS5 search
- Theme engine with 7 presets + TOML overrides

### 5. Quick Start
- Build, run, run with theme, run tests
- CLI options table (--debug, --theme, --config-dir)

### 6. Project Status
- Phase table (1-6) with completion status
- Honest note about Tor transport being stubbed — crypto, DB, UI, protocol all real

### 7. Project Structure
- Clean tree view of src/ with one-line descriptions per file/directory

### 8. Footer
- Requirements: Rust 1.70+, Linux/macOS, SQLCipher bundled
- License: MIT OR Apache-2.0

## Cleanup

Delete stale docs:
- `TESTING.md` — Phase 2 testing guide, references stubs that no longer exist
- `MIGRATION_FIX.md` — Phase 1→2 migration fix, schema is now v7

## Key Corrections from Current README

- Friend codes are now 32-word mnemonics (not `word-NNNN-word-NNNN` with checksum)
- Signal Protocol is real (not stubbed)
- Phases 3-5 are complete (channels, theming, crypto hardening)
- Database schema is v7 (not v2)
- 209+ tests passing (not 52)
- Vanity mining, theming, broadcast channels all exist now
