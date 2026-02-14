# Project Polish — Design

## Overview

Add license files and shell completions. Small, non-conflicting improvements while the Tor implementation is in flight.

## Deliverable 1: License Files

Add `LICENSE-MIT` and `LICENSE-APACHE` to the repo root with standard boilerplate text. Matches the `license = "MIT OR Apache-2.0"` declaration in `Cargo.toml`.

## Deliverable 2: Shell Completions

Add `clap_complete` as a build dependency. Create a `build.rs` that generates completion scripts for bash, zsh, and fish. Check generated files into `completions/` so users can install them without building from source.

Files created:
- `build.rs` — generates completions from clap Command
- `completions/chattor.bash`
- `completions/chattor.zsh`
- `completions/chattor.fish`

## Out of Scope

- CI/CD — deferred until compiler warnings are cleaned up after Tor refactoring
- Packaging — blocked on Tor implementation
- Cargo.toml author field fix — can be done anytime
