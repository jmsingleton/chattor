# Distribution Packaging Design

## Overview

Package chattor for four distribution channels: AUR (Arch Linux), Homebrew (macOS), deb (Debian/Ubuntu), and rpm (Fedora/RHEL). Automated via GitHub Actions on version tags.

## Release Flow

`git tag v0.2.0 && git push --tags` triggers `.github/workflows/release.yml`, which:

1. Builds release binaries on 3 runners: `ubuntu-latest` (x86_64 Linux), `macos-latest` (arm64 macOS), `macos-13` (x86_64 macOS)
2. Creates `.tar.gz` archives containing binary, man page, completions, and LICENSE files
3. Builds `.deb` and `.rpm` on the Linux runner using `cargo-deb` and `cargo-generate-rpm`
4. Creates a GitHub Release with all artifacts attached
5. Updates AUR packages (`chattor-bin` and `chattor-git`) via SSH push
6. Updates Homebrew tap via GitHub token

## Package Formats

### AUR — `chattor-bin`

Downloads prebuilt x86_64 Linux `.tar.gz` from GitHub release. Installs binary, man page, completions. No build dependencies beyond `tar`.

### AUR — `chattor-git`

Clones repo at HEAD, builds with `cargo build --release`. Build dependencies: `rust`, `gcc`, `perl` (for bundled SQLCipher). Installs same file set.

### Homebrew Formula

Published to `homebrew-chattor` tap repo. Builds from source tarball with `cargo install`. Handles arm64 and x86_64 macOS. Installs binary, man page, completions.

### deb (Debian/Ubuntu)

Built via `cargo-deb`. Reads `[package.metadata.deb]` section in `Cargo.toml`. Produces `.deb` with binary, man page, completions. Depends on `libc6`. Architecture: `amd64`.

### rpm (Fedora/RHEL)

Built via `cargo-generate-rpm`. Reads `[package.metadata.generate-rpm]` section in `Cargo.toml`. Same file set as deb. Architecture: `x86_64`.

## Installed Files (all formats)

- `/usr/bin/chattor`
- `/usr/share/man/man1/chattor.1`
- Bash completions: `/usr/share/bash-completion/completions/chattor`
- Zsh completions: `/usr/share/zsh/site-functions/_chattor`
- Fish completions: `/usr/share/fish/vendor_completions.d/chattor.fish`

## New Files

```
.github/workflows/release.yml       # CI: build, package, release, AUR update
dist/
  aur-bin/PKGBUILD                   # chattor-bin AUR package
  aur-git/PKGBUILD                   # chattor-git AUR package
  homebrew/chattor.rb                # Homebrew formula
```

## Cargo.toml Additions

```toml
[package.metadata.deb]
section = "net"
assets = [
    ["target/release/chattor", "usr/bin/", "755"],
    ["man/chattor.1", "usr/share/man/man1/", "644"],
    ["completions/chattor.bash", "usr/share/bash-completion/completions/chattor", "644"],
    ["completions/_chattor", "usr/share/zsh/site-functions/_chattor", "644"],
    ["completions/chattor.fish", "usr/share/fish/vendor_completions.d/chattor.fish", "644"],
]
depends = "libc6"

[package.metadata.generate-rpm]
assets = [
    { source = "target/release/chattor", dest = "/usr/bin/chattor", mode = "755" },
    { source = "man/chattor.1", dest = "/usr/share/man/man1/chattor.1", mode = "644" },
    { source = "completions/chattor.bash", dest = "/usr/share/bash-completion/completions/chattor", mode = "644" },
    { source = "completions/_chattor", dest = "/usr/share/zsh/site-functions/_chattor", mode = "644" },
    { source = "completions/chattor.fish", dest = "/usr/share/fish/vendor_completions.d/chattor.fish", mode = "644" },
]
requires = { libc = "*" }
```

## AUR Publishing

CI clones AUR repos (`aur:chattor-bin.git`, `aur:chattor-git.git`) via SSH, updates PKGBUILDs with new version and checksums, generates `.SRCINFO`, and pushes. SSH deploy key stored as GitHub Actions secret (`AUR_SSH_KEY`).

## Homebrew Publishing

CI pushes updated formula to `homebrew-chattor` tap repo via `HOMEBREW_TAP_TOKEN` secret (GitHub PAT with repo scope on the tap).
