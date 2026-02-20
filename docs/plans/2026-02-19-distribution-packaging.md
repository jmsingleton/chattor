# Distribution Packaging Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Package chattor for AUR (bin + git), Homebrew, deb, and rpm with automated GitHub Actions releases on version tags.

**Architecture:** Add `cargo-deb` and `cargo-generate-rpm` metadata to `Cargo.toml`, create PKGBUILD files for AUR, a Homebrew formula template, and a GitHub Actions workflow that builds on Linux + macOS, packages all formats, creates a GitHub Release, and pushes updates to AUR and the Homebrew tap.

**Tech Stack:** GitHub Actions, cargo-deb, cargo-generate-rpm, makepkg, Homebrew

---

### Task 1: Add deb and rpm metadata to Cargo.toml

**Files:**
- Modify: `Cargo.toml` (append after `[build-dependencies]` section, line 78)

**Step 1: Add packaging metadata**

Append the following to `Cargo.toml`:

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
```

**Step 2: Verify Cargo.toml still parses**

Run: `cargo metadata --format-version 1 --no-deps | head -1`
Expected: JSON output starting with `{"packages":[`

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add cargo-deb and cargo-generate-rpm metadata"
```

---

### Task 2: Create AUR chattor-bin PKGBUILD

**Files:**
- Create: `dist/aur-bin/PKGBUILD`

**Step 1: Create the PKGBUILD**

```bash
# Maintainer: John Singleton <john@example.com>
pkgname=chattor-bin
pkgver=0.1.0
pkgrel=1
pkgdesc="Privacy-first TUI chat application over Tor"
arch=('x86_64')
url="https://github.com/jmsingleton/chattor"
license=('MIT' 'Apache-2.0')
depends=('gcc-libs')
provides=('chattor')
conflicts=('chattor' 'chattor-git')
source=("${url}/releases/download/v${pkgver}/chattor-${pkgver}-x86_64-linux.tar.gz")
sha256sums=('SKIP')

package() {
    install -Dm755 chattor -t "${pkgdir}/usr/bin/"
    install -Dm644 chattor.1 "${pkgdir}/usr/share/man/man1/chattor.1"
    install -Dm644 completions/chattor.bash "${pkgdir}/usr/share/bash-completion/completions/chattor"
    install -Dm644 completions/_chattor "${pkgdir}/usr/share/zsh/site-functions/_chattor"
    install -Dm644 completions/chattor.fish "${pkgdir}/usr/share/fish/vendor_completions.d/chattor.fish"
    install -Dm644 LICENSE-MIT "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE-MIT"
    install -Dm644 LICENSE-APACHE "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE-APACHE"
}
```

**Step 2: Validate PKGBUILD syntax**

Run: `bash -n dist/aur-bin/PKGBUILD`
Expected: no output (syntax OK)

**Step 3: Commit**

```bash
git add dist/aur-bin/PKGBUILD
git commit -m "chore: add AUR chattor-bin PKGBUILD"
```

---

### Task 3: Create AUR chattor-git PKGBUILD

**Files:**
- Create: `dist/aur-git/PKGBUILD`

**Step 1: Create the PKGBUILD**

```bash
# Maintainer: John Singleton <john@example.com>
pkgname=chattor-git
pkgver=0.1.0
pkgrel=1
pkgdesc="Privacy-first TUI chat application over Tor (git version)"
arch=('x86_64')
url="https://github.com/jmsingleton/chattor"
license=('MIT' 'Apache-2.0')
makedepends=('rust' 'cargo' 'gcc' 'perl')
depends=('gcc-libs')
provides=('chattor')
conflicts=('chattor' 'chattor-bin')
source=("git+${url}.git")
sha256sums=('SKIP')

pkgver() {
    cd chattor
    git describe --tags --long 2>/dev/null | sed 's/^v//;s/-/.r/;s/-/./' || echo "$pkgver"
}

build() {
    cd chattor/chattor
    cargo build --release --locked
}

package() {
    cd chattor/chattor
    install -Dm755 target/release/chattor -t "${pkgdir}/usr/bin/"
    install -Dm644 man/chattor.1 "${pkgdir}/usr/share/man/man1/chattor.1"
    install -Dm644 completions/chattor.bash "${pkgdir}/usr/share/bash-completion/completions/chattor"
    install -Dm644 completions/_chattor "${pkgdir}/usr/share/zsh/site-functions/_chattor"
    install -Dm644 completions/chattor.fish "${pkgdir}/usr/share/fish/vendor_completions.d/chattor.fish"
    install -Dm644 LICENSE-MIT "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE-MIT"
    install -Dm644 LICENSE-APACHE "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE-APACHE"
}
```

**Step 2: Validate PKGBUILD syntax**

Run: `bash -n dist/aur-git/PKGBUILD`
Expected: no output (syntax OK)

**Step 3: Commit**

```bash
git add dist/aur-git/PKGBUILD
git commit -m "chore: add AUR chattor-git PKGBUILD"
```

---

### Task 4: Create Homebrew formula

**Files:**
- Create: `dist/homebrew/chattor.rb`

**Step 1: Create the formula**

```ruby
class Chattor < Formula
  desc "Privacy-first TUI chat application over Tor"
  homepage "https://github.com/jmsingleton/chattor"
  url "https://github.com/jmsingleton/chattor/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER"
  license any_of: ["MIT", "Apache-2.0"]

  depends_on "rust" => :build

  def install
    cd "chattor" do
      system "cargo", "install", *std_cargo_args
      man1.install "man/chattor.1"
      bash_completion.install "completions/chattor.bash" => "chattor"
      zsh_completion.install "completions/_chattor"
      fish_completion.install "completions/chattor.fish"
    end
  end

  test do
    assert_match "chattor", shell_output("#{bin}/chattor --help")
  end
end
```

**Step 2: Verify Ruby syntax**

Run: `ruby -c dist/homebrew/chattor.rb`
Expected: `Syntax of dist/homebrew/chattor.rb is OK`

**Step 3: Commit**

```bash
git add dist/homebrew/chattor.rb
git commit -m "chore: add Homebrew formula"
```

---

### Task 5: Create GitHub Actions release workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Step 1: Create the workflow**

The workflow triggers on `v*` tags and has 3 jobs:

1. **build** — matrix strategy across `ubuntu-latest`, `macos-latest`, `macos-13`. Each builds a release binary, creates a `.tar.gz` archive with binary + man page + completions + licenses. The Linux runner additionally builds `.deb` (via `cargo-deb`) and `.rpm` (via `cargo-generate-rpm`).

2. **release** — collects all artifacts from build matrix, creates a GitHub Release with all `.tar.gz`, `.deb`, and `.rpm` files attached.

3. **publish-aur** — runs after release. Sets up SSH with `AUR_SSH_KEY` secret. For `chattor-bin`: clones `ssh://aur@aur.archlinux.org/chattor-bin.git`, updates `pkgver` and `sha256sums` in PKGBUILD from the release tarball checksum, generates `.SRCINFO` via `makepkg --printsrcinfo`, commits and pushes. For `chattor-git`: clones `ssh://aur@aur.archlinux.org/chattor-git.git`, updates `pkgver`, generates `.SRCINFO`, commits and pushes.

4. **publish-homebrew** — runs after release. Clones `jmsingleton/homebrew-chattor` via `HOMEBREW_TAP_TOKEN`, updates `url` and `sha256` in the formula from the source tarball, commits and pushes.

```yaml
name: Release

on:
  push:
    tags: ['v*']

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            suffix: x86_64-linux
          - os: macos-latest
            target: aarch64-apple-darwin
            suffix: aarch64-macos
          - os: macos-13
            target: x86_64-apple-darwin
            suffix: x86_64-macos
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Build release binary
        working-directory: chattor
        run: cargo build --release --target ${{ matrix.target }}

      - name: Create tarball
        working-directory: chattor
        run: |
          VERSION="${GITHUB_REF_NAME#v}"
          ARCHIVE="chattor-${VERSION}-${{ matrix.suffix }}.tar.gz"
          mkdir -p staging
          cp target/${{ matrix.target }}/release/chattor staging/
          cp man/chattor.1 staging/
          cp -r completions staging/
          cp LICENSE-MIT LICENSE-APACHE staging/
          tar -czf "${ARCHIVE}" -C staging .
          echo "ARCHIVE=${ARCHIVE}" >> $GITHUB_ENV

      - name: Build .deb package
        if: matrix.os == 'ubuntu-latest'
        working-directory: chattor
        run: |
          cargo install cargo-deb
          cargo deb --target ${{ matrix.target }} --no-build

      - name: Build .rpm package
        if: matrix.os == 'ubuntu-latest'
        working-directory: chattor
        run: |
          cargo install cargo-generate-rpm
          cargo generate-rpm --target ${{ matrix.target }}

      - name: Upload tarball
        uses: actions/upload-artifact@v4
        with:
          name: archive-${{ matrix.suffix }}
          path: chattor/${{ env.ARCHIVE }}

      - name: Upload .deb
        if: matrix.os == 'ubuntu-latest'
        uses: actions/upload-artifact@v4
        with:
          name: deb
          path: chattor/target/${{ matrix.target }}/debian/*.deb

      - name: Upload .rpm
        if: matrix.os == 'ubuntu-latest'
        uses: actions/upload-artifact@v4
        with:
          name: rpm
          path: chattor/target/${{ matrix.target }}/generate-rpm/*.rpm

  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          files: artifacts/*

  publish-aur:
    needs: release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download Linux tarball
        uses: actions/download-artifact@v4
        with:
          name: archive-x86_64-linux
          path: artifacts

      - name: Setup SSH for AUR
        run: |
          mkdir -p ~/.ssh
          echo "${{ secrets.AUR_SSH_KEY }}" > ~/.ssh/aur
          chmod 600 ~/.ssh/aur
          echo "Host aur.archlinux.org" >> ~/.ssh/config
          echo "  IdentityFile ~/.ssh/aur" >> ~/.ssh/config
          echo "  User aur" >> ~/.ssh/config
          ssh-keyscan aur.archlinux.org >> ~/.ssh/known_hosts

      - name: Compute checksum
        run: |
          SHA256=$(sha256sum artifacts/chattor-*-x86_64-linux.tar.gz | cut -d' ' -f1)
          echo "SHA256=${SHA256}" >> $GITHUB_ENV
          VERSION="${GITHUB_REF_NAME#v}"
          echo "VERSION=${VERSION}" >> $GITHUB_ENV

      - name: Update chattor-bin
        run: |
          git clone ssh://aur@aur.archlinux.org/chattor-bin.git /tmp/chattor-bin
          cp chattor/dist/aur-bin/PKGBUILD /tmp/chattor-bin/PKGBUILD
          cd /tmp/chattor-bin
          sed -i "s/pkgver=.*/pkgver=${VERSION}/" PKGBUILD
          sed -i "s/sha256sums=.*/sha256sums=('${SHA256}')/" PKGBUILD
          docker run --rm -v /tmp/chattor-bin:/pkg archlinux bash -c "cd /pkg && makepkg --printsrcinfo > .SRCINFO"
          git add PKGBUILD .SRCINFO
          git commit -m "Update to v${VERSION}"
          git push

      - name: Update chattor-git
        run: |
          git clone ssh://aur@aur.archlinux.org/chattor-git.git /tmp/chattor-git
          cp chattor/dist/aur-git/PKGBUILD /tmp/chattor-git/PKGBUILD
          cd /tmp/chattor-git
          sed -i "s/pkgver=.*/pkgver=${VERSION}/" PKGBUILD
          docker run --rm -v /tmp/chattor-git:/pkg archlinux bash -c "cd /pkg && makepkg --printsrcinfo > .SRCINFO"
          git add PKGBUILD .SRCINFO
          git commit -m "Update to v${VERSION}"
          git push

  publish-homebrew:
    needs: release
    runs-on: ubuntu-latest
    steps:
      - name: Get source tarball checksum
        run: |
          VERSION="${GITHUB_REF_NAME#v}"
          URL="https://github.com/jmsingleton/chattor/archive/refs/tags/${GITHUB_REF_NAME}.tar.gz"
          curl -sL "${URL}" -o source.tar.gz
          SHA256=$(sha256sum source.tar.gz | cut -d' ' -f1)
          echo "SHA256=${SHA256}" >> $GITHUB_ENV
          echo "VERSION=${VERSION}" >> $GITHUB_ENV
          echo "URL=${URL}" >> $GITHUB_ENV

      - name: Update Homebrew tap
        run: |
          git clone https://x-access-token:${{ secrets.HOMEBREW_TAP_TOKEN }}@github.com/jmsingleton/homebrew-chattor.git /tmp/tap
          mkdir -p /tmp/tap/Formula
          cp chattor/dist/homebrew/chattor.rb /tmp/tap/Formula/chattor.rb
          cd /tmp/tap
          sed -i "s|url \".*\"|url \"${URL}\"|" Formula/chattor.rb
          sed -i "s/sha256 \".*\"/sha256 \"${SHA256}\"/" Formula/chattor.rb
          git add Formula/chattor.rb
          git commit -m "chattor ${VERSION}"
          git push
```

**Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"`
Expected: no output (valid YAML)

**Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow for deb, rpm, AUR, and Homebrew"
```

---

### Task 6: Add Cargo.lock to version control

**Context:** The `Cargo.lock` file is already tracked (visible in `ls` output). Verify it's committed and up to date. Release builds with `--locked` require it.

**Step 1: Verify Cargo.lock is tracked**

Run: `git ls-files Cargo.lock`
Expected: `Cargo.lock`

**Step 2: If not tracked, add it**

Run: `git add Cargo.lock` (only if step 1 shows nothing)

---

### Task 7: Final verification and commit

**Step 1: Verify full build still works**

Run: `cargo build --release 2>&1 | tail -3`
Expected: `Finished` line

**Step 2: Verify tests pass**

Run: `cargo test 2>&1 | tail -5`
Expected: all tests pass

**Step 3: Verify all new files exist**

Run: `ls -la dist/aur-bin/PKGBUILD dist/aur-git/PKGBUILD dist/homebrew/chattor.rb .github/workflows/release.yml`
Expected: all 4 files listed

**Step 4: Review git log**

Run: `git log --oneline -10`
Expected: 4-5 new commits for packaging work
