---
title: Installation
description: How to install chattor on your system
---

## From Source (Recommended)

chattor is built with Rust. You'll need [Rust 1.70+](https://rustup.rs/) installed.

```bash
git clone https://github.com/jmsingleton/chattor.git
cd chattor
cargo build --release
```

The binary will be at `target/release/chattor`.

## Package Managers

### Arch Linux (AUR)

```bash
# Binary release
yay -S chattor-bin

# Build from source
yay -S chattor-git
```

### Debian / Ubuntu

Download the `.deb` from the [latest release](https://github.com/jmsingleton/chattor/releases):

```bash
sudo dpkg -i chattor_*.deb
```

### Fedora / RHEL

Download the `.rpm` from the [latest release](https://github.com/jmsingleton/chattor/releases):

```bash
sudo rpm -i chattor-*.rpm
```

### Homebrew (macOS)

```bash
brew tap jmsingleton/chattor
brew install chattor
```

## Requirements

- **Platform**: Linux, macOS, BSD (no Windows support)
- **Dependencies**: None — SQLCipher is bundled via `rusqlite`, Tor is embedded via `arti`
- **Rust**: 1.70+ (edition 2021) if building from source
