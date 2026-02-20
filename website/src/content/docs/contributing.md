---
title: Contributing
description: How to build, test, and contribute to chattor
---

## Building from Source

```bash
git clone https://github.com/jmsingleton/chattor.git
cd chattor
cargo build
```

## Running Tests

```bash
# All tests
cargo test

# Specific module
cargo test protocol::message

# Integration tests only
cargo test --test integration

# E2E crypto/messaging tests
cargo test --test e2e_messaging

# With output
cargo test -- --nocapture
```

## Code Quality

```bash
# Format
cargo fmt

# Lint
cargo clippy -- -D warnings
```

## Project Structure

See the [Architecture Overview](/architecture/overview/) for a guide to the codebase.

## Testing Strategy

- **Unit tests**: Per-module in `#[cfg(test)]` blocks
- **Integration tests**: `tests/integration/` for cross-module interaction
- **E2E tests**: `tests/e2e_messaging.rs` for full Signal Protocol pipeline
- **Database tests**: Use `tempfile` crate for isolated test databases

## Submitting Changes

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes with tests
4. Run `cargo fmt` and `cargo clippy -- -D warnings`
5. Run `cargo test` and ensure all tests pass
6. Submit a pull request

## License

chattor is dual-licensed under MIT and Apache-2.0.
