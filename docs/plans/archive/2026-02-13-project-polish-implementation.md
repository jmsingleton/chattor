# Project Polish Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add license files and shell completions to polish the project for distribution.

**Architecture:** Standard MIT/Apache-2.0 dual license files at repo root. Shell completions generated via `clap_complete` in a `build.rs` script, with output checked into `completions/`.

**Tech Stack:** `clap_complete` (new build dependency), `build.rs`, ROFF-style license boilerplate

---

### Task 1: Add license files

**Files:**
- Create: `LICENSE-MIT`
- Create: `LICENSE-APACHE`

**Step 1: Create LICENSE-MIT**

Write the standard MIT license text to `LICENSE-MIT`. Use "chattor contributors" as the copyright holder (standard for open-source Rust projects that don't want to enumerate individuals). Year: 2026.

```
MIT License

Copyright (c) 2026 chattor contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

**Step 2: Create LICENSE-APACHE**

Write the full Apache License 2.0 text to `LICENSE-APACHE`. The canonical text is available at https://www.apache.org/licenses/LICENSE-2.0.txt — use the standard text verbatim.

**Step 3: Verify both files exist**

```bash
ls -la LICENSE-MIT LICENSE-APACHE
```

**Step 4: Commit**

```bash
git add LICENSE-MIT LICENSE-APACHE
git commit -m "chore: add MIT and Apache-2.0 license files"
```

---

### Task 2: Add clap_complete build dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add clap_complete to build-dependencies**

Read `Cargo.toml`. Add a `[build-dependencies]` section (it doesn't exist yet) with `clap_complete` and `clap` (needed to reconstruct the Command in build.rs):

```toml
[build-dependencies]
clap = { version = "4.5", features = ["derive"] }
clap_complete = "4"
```

Note: `clap` is already in `[dependencies]` — it must also appear in `[build-dependencies]` because `build.rs` runs in a separate compilation context and can't use regular dependencies.

**Step 2: Verify it compiles**

```bash
cargo check
```

Expected: compiles successfully (build.rs doesn't exist yet, so build-deps are just downloaded).

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add clap_complete build dependency for shell completions"
```

---

### Task 3: Create build.rs for shell completion generation

**Files:**
- Create: `build.rs`
- Create: `completions/` directory (created by build.rs)

**Step 1: Read src/cli.rs to understand the Command structure**

Read `/home/john/chattor/chattor/src/cli.rs`. The `Cli` struct uses clap derive macros. In `build.rs`, you need to call `Cli::command()` to get the `Command`, then pass it to `clap_complete::generate()`.

IMPORTANT: `build.rs` cannot import from `src/` directly. Instead, you must reconstruct the Command manually in `build.rs` using clap's builder API, OR use `include!` to pull in the struct. The cleanest approach for a simple CLI like this is to reconstruct it manually:

```rust
use clap::Command;
```

The command has:
- name: "chattor"
- 3 args: `--debug` (`-d`, bool), `--config-dir` (`-c`, takes value), `--theme` (`-t`, takes value)

**Step 2: Write build.rs**

Create `build.rs` at the project root:

```rust
use clap::{Arg, Command};
use clap_complete::{generate, Shell};
use std::fs;
use std::io::BufWriter;

fn build_cli() -> Command {
    Command::new("chattor")
        .about("Privacy-first TUI chat application over Tor")
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .action(clap::ArgAction::SetTrue)
                .help("Enable debug logging"),
        )
        .arg(
            Arg::new("config-dir")
                .short('c')
                .long("config-dir")
                .value_name("PATH")
                .help("Config directory path"),
        )
        .arg(
            Arg::new("theme")
                .short('t')
                .long("theme")
                .value_name("NAME")
                .help("Theme preset (dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn)"),
        )
}

fn main() {
    let outdir = std::path::PathBuf::from("completions");
    fs::create_dir_all(&outdir).unwrap();

    let mut cmd = build_cli();

    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
        let filename = match shell {
            Shell::Bash => "chattor.bash",
            Shell::Zsh => "_chattor",
            Shell::Fish => "chattor.fish",
            _ => unreachable!(),
        };
        let path = outdir.join(filename);
        let mut file = BufWriter::new(fs::File::create(&path).unwrap());
        generate(shell, &mut cmd, "chattor", &mut file);
    }
}
```

Note on zsh: the convention is `_chattor` (underscore prefix) for zsh completions.

**Step 3: Build to generate completions**

```bash
cargo build
```

Expected: `completions/` directory created with three files.

**Step 4: Verify generated files exist and look reasonable**

```bash
ls -la completions/
head -5 completions/chattor.bash
head -5 completions/_chattor
head -5 completions/chattor.fish
```

Expected: each file contains shell-specific completion code referencing `chattor`, `--debug`, `--theme`, `--config-dir`.

**Step 5: Commit**

```bash
git add build.rs completions/
git commit -m "feat: add shell completions for bash, zsh, and fish via clap_complete"
```

---

### Task 4: Add completions install instructions to man page

**Files:**
- Modify: `man/chattor.1`

**Step 1: Read the current man page**

Read `/home/john/chattor/chattor/man/chattor.1` and find the FILES section.

**Step 2: Add a SHELL COMPLETIONS section before FILES**

Add a new section between THEMES and FILES:

```roff
.SH SHELL COMPLETIONS
Shell completion scripts are included in the
.B completions/
directory of the source distribution.
.TP
.B Bash
Copy
.I completions/chattor.bash
to
.I ~/.local/share/bash-completion/completions/chattor
.TP
.B Zsh
Copy
.I completions/_chattor
to a directory in your
.BR $fpath .
.TP
.B Fish
Copy
.I completions/chattor.fish
to
.I ~/.config/fish/completions/
```

**Step 3: Verify man page renders**

```bash
man ./man/chattor.1
```

Check that the SHELL COMPLETIONS section appears and formats correctly.

**Step 4: Commit**

```bash
git add man/chattor.1
git commit -m "docs: add shell completions install instructions to man page"
```

---

### Task 5: Final verification

**Step 1: Verify completions work in bash**

```bash
source completions/chattor.bash
chattor --<TAB>
```

Expected: shows `--debug`, `--theme`, `--config-dir`, `--help`.

(If not in bash, just verify the file contents look correct.)

**Step 2: Verify license files exist**

```bash
head -3 LICENSE-MIT
head -3 LICENSE-APACHE
```

**Step 3: Verify build still works**

```bash
cargo build 2>&1 | tail -3
```

**Step 4: List all new files**

```bash
git log --oneline 69c8bd4..HEAD
```

Verify 4 commits: licenses, Cargo.toml dep, build.rs + completions, man page update.
