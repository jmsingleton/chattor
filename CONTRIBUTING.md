# Contributing to chattor

Thanks for your interest in chattor! This document covers everything you need to
get started.

## Quick Start

```bash
# Clone and build
git clone https://github.com/jmsingleton/chattor.git
cd chattor/chattor
cargo build

# Run the TUI
cargo run

# Run tests
cargo test
```

## Development Setup

**Requirements:**
- Rust 1.75+ (we use 2021 edition)
- A C compiler (for SQLCipher's bundled build)
- Linux or macOS (no Windows support)

**Optional:**
- `tor` or `arti` binary for end-to-end testing
- `wl-copy`, `xclip`, `xsel`, or `pbcopy` for clipboard support

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` to catch common issues
- Follow existing patterns in the codebase — when in doubt, match what's nearby
- Use `crate::error::Result<T>` for fallible functions
- Propagate errors with `?`, wrap external errors with `.map_err()`

## Testing

```bash
cargo test                        # All tests
cargo test protocol::message      # Specific module
cargo test --test integration     # Integration tests only
cargo test -- --nocapture         # Show stdout/stderr
```

Tests use `tempfile::NamedTempFile` for isolated database instances. Each test
gets its own database — no shared state, no cleanup needed.

## Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add channel post pagination
fix: handle empty friend code input
docs: update keybindings in man page
chore: bump ratatui to 0.28
refactor: extract message envelope into its own module
test: add integration test for offline message queue
```

Keep the subject line under 72 characters. Use the body for context about *why*
the change was made, not *what* changed (the diff shows that).

## Pull Requests

1. Create a feature branch from `main`
2. Make your changes with clear, focused commits
3. Ensure `cargo test`, `cargo fmt -- --check`, and `cargo clippy` all pass
4. Open a PR with a description of what changed and why

## Architecture Notes

Before diving in, skim the `CLAUDE.md` file — it has a full architecture
overview, module descriptions, and common patterns. Key things to know:

- **`src/app.rs`** is the central application state
- **`src/db/`** handles all persistence (SQLCipher, schema migrations)
- **`src/crypto/`** handles identity and Signal Protocol encryption
- **`src/protocol/`** defines the wire format (12 message types)
- **`src/ui/`** is the TUI layer (ratatui)
- **`src/tor/`** is the Tor integration layer

## Security

If you find a security vulnerability, please follow the process in
[SECURITY.md](SECURITY.md) instead of opening a public issue.

## License

By contributing, you agree that your contributions will be licensed under the
same terms as the project: MIT OR Apache-2.0 (your choice).
