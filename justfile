# chattor development recipes
# Install just: cargo install just

default:
    @just --list

# Build in debug mode
build:
    cargo build

# Build in release mode
release:
    cargo build --release

# Run the TUI
run *ARGS:
    cargo run -- {{ARGS}}

# Run all tests
test:
    cargo test

# Run a specific test module
test-mod MOD:
    cargo test {{MOD}}

# Run integration tests only
test-integration:
    cargo test --test integration

# Check formatting and lint
check:
    cargo fmt -- --check
    cargo clippy

# Format code
fmt:
    cargo fmt

# Install binary, man page, and completions (Linux)
install: release
    install -Dm755 target/release/chattor ~/.local/bin/chattor
    install -Dm644 man/chattor.1 ~/.local/share/man/man1/chattor.1
    install -Dm644 completions/chattor.bash ~/.local/share/bash-completion/completions/chattor
    install -Dm644 completions/chattor.fish ~/.config/fish/completions/chattor.fish

# Uninstall
uninstall:
    rm -f ~/.local/bin/chattor
    rm -f ~/.local/share/man/man1/chattor.1
    rm -f ~/.local/share/bash-completion/completions/chattor
    rm -f ~/.config/fish/completions/chattor.fish

# View the man page
man:
    man ./man/chattor.1

# Run with debug logging to a file
debug:
    cargo run -- --debug 2>chattor.log
