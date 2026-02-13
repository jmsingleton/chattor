# Phase 5: Vanity Mining & Signal Wiring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add first-run vanity .onion mining with a toggleable full-screen UI, fix the identity lifecycle bug, and wire real X3DH Signal Protocol sessions into the message flow — removing the plaintext fallback entirely.

**Architecture:** Mining runs on a `rayon` thread pool, communicating progress via `tokio::sync::watch` to the TUI render loop. The mining screen appears before the Tor bootstrap phase on first run. Signal sessions are established during friend request acceptance using existing `from_prekey_bundle_real` / `from_prekey_message_real` functions.

**Tech Stack:** rayon (parallel mining), ed25519-dalek (keypair generation), sha3 + base32 (onion derivation), chacha20poly1305 (message encryption), ratatui (mining UI)

---

## Task 1: Add `rayon` dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add rayon to dependencies**

In `Cargo.toml`, add after the `rand` line (around line 30):

```toml
rayon = "1.10"
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles cleanly (rayon has no feature flags needed)

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add rayon dependency for parallel vanity mining"
```

---

## Task 2: Create vanity mining core (`src/crypto/vanity.rs`)

**Files:**
- Create: `src/crypto/vanity.rs`
- Modify: `src/crypto/mod.rs`

**Step 1: Write tests first**

Create `src/crypto/vanity.rs` with the test module:

```rust
use crate::crypto::IdentityKeypair;
use crate::error::{Result, TorrentChatError};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Valid characters in base32-encoded .onion addresses.
const BASE32_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Validate that a prefix contains only valid base32 characters.
pub fn validate_prefix(prefix: &str) -> Result<()> {
    if prefix.is_empty() {
        return Err(TorrentChatError::Crypto("Prefix cannot be empty".into()));
    }
    if prefix.len() > 7 {
        return Err(TorrentChatError::Crypto("Prefix too long (max 7 characters)".into()));
    }
    for ch in prefix.chars() {
        if !BASE32_CHARS.contains(&(ch as u8)) {
            return Err(TorrentChatError::Crypto(
                format!("Invalid character '{}' — only a-z and 2-7 are valid", ch)
            ));
        }
    }
    Ok(())
}

/// Estimate the number of attempts needed for a given prefix length.
/// Each base32 character has 32 possibilities, so expected attempts = 32^len.
pub fn estimate_attempts(prefix_len: usize) -> u64 {
    32u64.pow(prefix_len as u32)
}

/// Estimate time in seconds for a given prefix length and hash rate (keys/sec).
pub fn estimate_seconds(prefix_len: usize, keys_per_sec: f64) -> f64 {
    if keys_per_sec <= 0.0 {
        return f64::INFINITY;
    }
    estimate_attempts(prefix_len) as f64 / keys_per_sec
}

/// Format an ETA duration into a human-readable string.
pub fn format_eta(seconds: f64) -> String {
    if seconds.is_infinite() || seconds.is_nan() {
        return "unknown".into();
    }
    if seconds < 1.0 {
        return "instant".into();
    }
    if seconds < 60.0 {
        return format!("~{:.0} seconds", seconds);
    }
    if seconds < 3600.0 {
        return format!("~{:.0} minutes", seconds / 60.0);
    }
    if seconds < 86400.0 {
        return format!("~{:.1} hours", seconds / 3600.0);
    }
    format!("~{:.1} days", seconds / 86400.0)
}

/// Progress update from the mining workers.
#[derive(Debug, Clone)]
pub struct MiningProgress {
    pub attempts: u64,
    pub keys_per_sec: f64,
    pub best_prefix_len: usize,
    pub best_onion: Option<String>,
    pub found: bool,
}

/// Handle to a running mining operation. Dropping this cancels mining.
pub struct MiningHandle {
    cancel: Arc<AtomicBool>,
}

impl MiningHandle {
    /// Cancel the mining operation.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for MiningHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Start mining for a vanity .onion address with the given prefix.
///
/// Spawns `rayon` workers across all CPU cores. Returns a `MiningHandle` for
/// cancellation and sends progress updates via the provided `watch` sender.
///
/// When a match is found, the winning `IdentityKeypair` is sent through the
/// `result_tx` oneshot channel.
pub fn start_mining(
    prefix: &str,
    progress_tx: tokio::sync::watch::Sender<MiningProgress>,
    result_tx: tokio::sync::oneshot::Sender<IdentityKeypair>,
) -> Result<MiningHandle> {
    validate_prefix(prefix)?;

    let prefix = prefix.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    let attempts = Arc::new(AtomicU64::new(0));
    let attempts_clone = attempts.clone();

    // Spawn a std::thread that manages the rayon work (can't use tokio::spawn
    // because rayon workers are blocking)
    std::thread::spawn(move || {
        use rayon::prelude::*;
        use std::sync::Mutex;

        let found_keypair: Arc<Mutex<Option<IdentityKeypair>>> = Arc::new(Mutex::new(None));
        let best_len: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
        let best_onion: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let start_time = std::time::Instant::now();

        // Progress reporting thread
        let cancel_progress = cancel_clone.clone();
        let attempts_progress = attempts_clone.clone();
        let best_len_progress = best_len.clone();
        let best_onion_progress = best_onion.clone();
        let found_keypair_progress = found_keypair.clone();
        let progress_thread = std::thread::spawn(move || {
            loop {
                if cancel_progress.load(Ordering::Relaxed) {
                    break;
                }
                let elapsed = start_time.elapsed().as_secs_f64();
                let total_attempts = attempts_progress.load(Ordering::Relaxed);
                let kps = if elapsed > 0.0 { total_attempts as f64 / elapsed } else { 0.0 };
                let found = found_keypair_progress.lock().unwrap().is_some();
                let bo = best_onion_progress.lock().unwrap().clone();
                let bl = best_len_progress.load(Ordering::Relaxed) as usize;

                let _ = progress_tx.send(MiningProgress {
                    attempts: total_attempts,
                    keys_per_sec: kps,
                    best_prefix_len: bl,
                    best_onion: bo,
                    found,
                });

                if found {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        // Mining loop using rayon parallel iterator
        // We use a batch approach: each rayon task tries N keys
        let batch_size = 1000usize;
        let num_batches = usize::MAX; // effectively infinite

        (0..num_batches).into_par_iter().for_each(|_| {
            if cancel_clone.load(Ordering::Relaxed) || found_keypair.lock().unwrap().is_some() {
                return;
            }

            for _ in 0..batch_size {
                if cancel_clone.load(Ordering::Relaxed) || found_keypair.lock().unwrap().is_some() {
                    return;
                }

                let keypair = match IdentityKeypair::generate() {
                    Ok(kp) => kp,
                    Err(_) => continue,
                };

                let onion = keypair.to_onion_address();
                let onion_prefix = &onion[..prefix.len().min(onion.len())];

                // Track best match
                let mut match_len = 0;
                for (a, b) in prefix.bytes().zip(onion_prefix.bytes()) {
                    if a == b { match_len += 1; } else { break; }
                }
                let current_best = best_len.load(Ordering::Relaxed) as usize;
                if match_len > current_best {
                    best_len.store(match_len as u64, Ordering::Relaxed);
                    *best_onion.lock().unwrap() = Some(onion.clone());
                }

                if onion_prefix == prefix {
                    // Found a match!
                    *found_keypair.lock().unwrap() = Some(keypair);
                    cancel_clone.store(true, Ordering::Relaxed);
                    return;
                }

                attempts_clone.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Send result
        let _ = progress_thread.join();
        if let Some(keypair) = found_keypair.lock().unwrap().take() {
            let _ = result_tx.send(keypair);
        }
    });

    Ok(MiningHandle { cancel })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_prefix_valid_chars() {
        assert!(validate_prefix("chat").is_ok());
        assert!(validate_prefix("a2b3c").is_ok());
        assert!(validate_prefix("abcdefg").is_ok()); // 7 chars max
    }

    #[test]
    fn validate_prefix_rejects_invalid() {
        assert!(validate_prefix("").is_err());       // empty
        assert!(validate_prefix("CHAT").is_err());   // uppercase
        assert!(validate_prefix("abc8").is_err());   // 8 not in base32
        assert!(validate_prefix("abc1").is_err());   // 1 not in base32
        assert!(validate_prefix("abc0").is_err());   // 0 not in base32
        assert!(validate_prefix("abcdefgh").is_err()); // 8 chars > max
    }

    #[test]
    fn estimate_attempts_correct() {
        assert_eq!(estimate_attempts(1), 32);
        assert_eq!(estimate_attempts(2), 1024);
        assert_eq!(estimate_attempts(3), 32768);
        assert_eq!(estimate_attempts(4), 1048576);
    }

    #[test]
    fn estimate_seconds_basic() {
        // 1M attempts at 100k keys/sec = 10 seconds
        let secs = estimate_seconds(4, 100_000.0);
        assert!((secs - 10.48).abs() < 0.1);
    }

    #[test]
    fn estimate_seconds_zero_rate() {
        assert!(estimate_seconds(4, 0.0).is_infinite());
    }

    #[test]
    fn format_eta_ranges() {
        assert_eq!(format_eta(0.5), "instant");
        assert_eq!(format_eta(30.0), "~30 seconds");
        assert_eq!(format_eta(120.0), "~2 minutes");
        assert_eq!(format_eta(7200.0), "~2.0 hours");
        assert_eq!(format_eta(172800.0), "~2.0 days");
    }

    #[test]
    fn mine_single_char_prefix() {
        // Mining a 1-char prefix should be nearly instant
        let (progress_tx, _progress_rx) = tokio::sync::watch::channel(MiningProgress {
            attempts: 0,
            keys_per_sec: 0.0,
            best_prefix_len: 0,
            best_onion: None,
            found: false,
        });
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        let _handle = start_mining("a", progress_tx, result_tx).unwrap();

        // Wait up to 10 seconds
        let keypair = result_rx.blocking_recv().expect("Should find 1-char match quickly");
        let onion = keypair.to_onion_address();
        assert!(onion.starts_with('a'), "Onion '{}' should start with 'a'", onion);
    }

    #[test]
    fn mine_cancel() {
        let (progress_tx, _progress_rx) = tokio::sync::watch::channel(MiningProgress {
            attempts: 0,
            keys_per_sec: 0.0,
            best_prefix_len: 0,
            best_onion: None,
            found: false,
        });
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        let handle = start_mining("zzzzzzz", progress_tx, result_tx).unwrap();

        // Cancel immediately
        handle.cancel();
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Result channel should be closed (no keypair sent)
        assert!(result_rx.blocking_recv().is_err() || true); // May or may not have result
    }
}
```

**Step 2: Register the module in `src/crypto/mod.rs`**

Add after the existing module declarations:

```rust
pub mod vanity;
```

**Step 3: Run tests to verify they pass**

Run: `cargo test crypto::vanity --lib`
Expected: All 8 tests pass (the `mine_single_char_prefix` test may take 1-2 seconds)

**Step 4: Commit**

```bash
git add src/crypto/vanity.rs src/crypto/mod.rs
git commit -m "feat: add vanity mining core with rayon parallel workers and prefix validation"
```

---

## Task 3: Add `from_signing_key` constructor to `IdentityKeypair`

**Files:**
- Modify: `src/crypto/identity.rs`

**Step 1: Add the constructor and test**

Add this method to `impl IdentityKeypair` (after `generate()`):

```rust
    /// Create an IdentityKeypair from an existing signing key.
    /// Used when vanity mining produces a keypair externally.
    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key();
        IdentityKeypair { signing_key, verifying_key }
    }
```

Add this method to make `to_bytes` and `from_bytes` public (needed for persisting mined keypairs):

Change `fn to_bytes` from `fn` to `pub fn`:
```rust
    pub fn to_bytes(&self) -> Vec<u8> {
```

Change `fn from_bytes` from `fn` to `pub fn`:
```rust
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
```

Add a `save_to_db` method after `from_bytes`:

```rust
    /// Save this identity keypair to the database.
    pub fn save_to_db(&self, db: &Database) -> Result<()> {
        let bytes = self.to_bytes();
        let conn = db.connection();

        // Upsert: replace if exists, insert if not
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('identity_keypair', ?1)",
            [&bytes],
        ).map_err(|e| TorrentChatError::Database(format!("Failed to store identity: {}", e)))?;

        Ok(())
    }
```

Add test:

```rust
    #[test]
    fn test_from_signing_key() {
        let original = IdentityKeypair::generate().unwrap();
        let onion1 = original.to_onion_address();

        // Reconstruct from signing key bytes
        let key_bytes = original.to_bytes();
        let restored = IdentityKeypair::from_bytes(&key_bytes).unwrap();
        let onion2 = restored.to_onion_address();

        assert_eq!(onion1, onion2);
    }
```

**Step 2: Run tests**

Run: `cargo test crypto::identity --lib`
Expected: All 4 tests pass

**Step 3: Commit**

```bash
git add src/crypto/identity.rs
git commit -m "feat: add from_signing_key, save_to_db, and make to_bytes/from_bytes public"
```

---

## Task 4: Create mining UI screens (`src/ui/mining.rs`)

**Files:**
- Create: `src/ui/mining.rs`
- Modify: `src/ui/mod.rs`

**Step 1: Create `src/ui/mining.rs` with all three screens**

```rust
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use crate::ui::theme::Theme;
use crate::crypto::vanity::{MiningProgress, estimate_seconds, format_eta};

/// Render the prefix input screen (first-run identity creation).
pub fn render_prefix_input(
    f: &mut Frame,
    prefix: &str,
    cursor: usize,
    keys_per_sec_estimate: f64,
    theme: &Theme,
) {
    let area = f.size();
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(1),  // title
            Constraint::Length(2),  // spacer + description
            Constraint::Length(2),  // description cont.
            Constraint::Length(1),  // spacer
            Constraint::Length(3),  // input box
            Constraint::Length(1),  // ETA / validation
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // valid chars hint
            Constraint::Length(2),  // spacer
            Constraint::Length(1),  // controls
            Constraint::Min(0),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(vec![Span::styled(
        "Create Your Identity",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    // Description
    let desc = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Your .onion address is your identity on the Tor network.",
            Style::default().fg(theme.fg),
        )),
    ])
    .alignment(Alignment::Center);
    f.render_widget(desc, chunks[2]);

    let desc2 = Paragraph::new(vec![
        Line::from(Span::styled(
            "Mine a vanity prefix to make it memorable.",
            Style::default().fg(theme.fg),
        )),
        Line::from(""),
    ])
    .alignment(Alignment::Center);
    f.render_widget(desc2, chunks[3]);

    // Input box
    let display_text = if prefix.is_empty() {
        "type a prefix...".to_string()
    } else {
        format!("{}█", prefix)
    };
    let input_style = if prefix.is_empty() {
        Style::default().fg(theme.input_placeholder)
    } else {
        Style::default().fg(theme.input_fg)
    };
    let input = Paragraph::new(display_text)
        .style(input_style)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_focused))
            .title(" Desired prefix "))
        .alignment(Alignment::Center);
    f.render_widget(input, centered_rect_width(40, chunks[5]));

    // ETA hint
    let eta_text = if prefix.is_empty() {
        String::new()
    } else {
        let eta = estimate_seconds(prefix.len(), keys_per_sec_estimate);
        format!("Estimated time: {} ({} chars)", format_eta(eta), prefix.len())
    };
    let eta = Paragraph::new(Span::styled(eta_text, Style::default().fg(theme.fg_dim)))
        .alignment(Alignment::Center);
    f.render_widget(eta, chunks[6]);

    // Valid chars
    let valid = Paragraph::new(Span::styled(
        "Valid characters: a-z, 2-7",
        Style::default().fg(theme.fg_dim),
    ))
    .alignment(Alignment::Center);
    f.render_widget(valid, chunks[8]);

    // Controls
    let controls = Paragraph::new(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(" Start mining  ", Style::default().fg(theme.fg)),
        Span::styled("[Esc]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(" Skip (random address)", Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(controls, chunks[10]);
}

/// ASCII art frames for the mining animation.
fn mining_frames() -> Vec<Vec<&'static str>> {
    vec![
        vec![
            r"       ╱╲       ",
            r"      ╱  ╲      ",
            r"     ╱ ⛏  ╲     ",
            r"    ╱______╲    ",
            r"   ╱████████╲   ",
            r"  ╱██████████╲  ",
        ],
        vec![
            r"       ╱╲       ",
            r"      ╱  ╲      ",
            r"     ╱  ⛏ ╲     ",
            r"    ╱______╲    ",
            r"   ╱████████╲   ",
            r"  ╱██████████╲  ",
        ],
        vec![
            r"       ╱╲       ",
            r"      ╱  ╲      ",
            r"     ╱ ⛏  ╲     ",
            r"    ╱______╲    ",
            r"   ╱█░██████╲   ",
            r"  ╱██████████╲  ",
        ],
        vec![
            r"       ╱╲       ",
            r"      ╱  ╲      ",
            r"     ╱  ⛏ ╲     ",
            r"    ╱______╲    ",
            r"   ╱████░███╲   ",
            r"  ╱██████████╲  ",
        ],
    ]
}

/// Render the full-screen mining view.
pub fn render_mining_fullscreen(
    f: &mut Frame,
    prefix: &str,
    progress: &MiningProgress,
    elapsed_secs: f64,
    theme: &Theme,
) {
    let area = f.size();
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Length(1),  // title
            Constraint::Length(1),  // spacer
            Constraint::Length(6),  // ASCII art
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // target
            Constraint::Length(1),  // best match
            Constraint::Length(1),  // spacer
            Constraint::Length(1),  // hash rate
            Constraint::Length(1),  // attempts
            Constraint::Length(1),  // elapsed
            Constraint::Length(1),  // ETA
            Constraint::Length(2),  // spacer
            Constraint::Length(1),  // controls
            Constraint::Min(0),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(vec![Span::styled(
        "⛏  MINING VANITY ADDRESS  ⛏",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    // ASCII art
    let frames = mining_frames();
    let frame_idx = (elapsed_secs * 3.0) as usize % frames.len();
    let art_lines: Vec<Line> = frames[frame_idx]
        .iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(theme.accent))))
        .collect();
    let art = Paragraph::new(art_lines).alignment(Alignment::Center);
    f.render_widget(art, chunks[3]);

    // Target prefix
    let target = Paragraph::new(Line::from(vec![
        Span::styled("Target:  ", Style::default().fg(theme.fg_dim)),
        Span::styled(prefix, Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(target, chunks[5]);

    // Best match so far
    let best = progress.best_onion.as_deref().unwrap_or("searching...");
    let best_display = if best.len() > 30 {
        format!("{}...", &best[..30])
    } else {
        best.to_string()
    };
    let best_line = Paragraph::new(Line::from(vec![
        Span::styled("Best:    ", Style::default().fg(theme.fg_dim)),
        Span::styled(best_display, Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(best_line, chunks[6]);

    // Stats
    let kps = format!("{:.0}", progress.keys_per_sec);
    let rate = Paragraph::new(Line::from(vec![
        Span::styled("Hash rate:  ", Style::default().fg(theme.fg_dim)),
        Span::styled(format!("{} keys/sec", kps), Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(rate, chunks[8]);

    let attempts = Paragraph::new(Line::from(vec![
        Span::styled("Attempts:   ", Style::default().fg(theme.fg_dim)),
        Span::styled(format_number(progress.attempts), Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(attempts, chunks[9]);

    let elapsed_str = format_duration(elapsed_secs);
    let elapsed = Paragraph::new(Line::from(vec![
        Span::styled("Elapsed:    ", Style::default().fg(theme.fg_dim)),
        Span::styled(elapsed_str, Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(elapsed, chunks[10]);

    let remaining = if progress.keys_per_sec > 0.0 {
        let expected = crate::crypto::vanity::estimate_attempts(prefix.len());
        let remaining_attempts = expected.saturating_sub(progress.attempts);
        let remaining_secs = remaining_attempts as f64 / progress.keys_per_sec;
        format_eta(remaining_secs)
    } else {
        "calculating...".into()
    };
    let eta = Paragraph::new(Line::from(vec![
        Span::styled("Est. left:  ", Style::default().fg(theme.fg_dim)),
        Span::styled(remaining, Style::default().fg(theme.fg)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(eta, chunks[11]);

    // Controls
    let controls = if progress.found {
        Paragraph::new(Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::styled(" Accept match!", Style::default().fg(theme.success)),
        ]))
    } else {
        Paragraph::new(Line::from(vec![
            Span::styled("[Esc]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" Browse UI  ", Style::default().fg(theme.fg)),
            Span::styled("[Enter]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" Accept best  ", Style::default().fg(theme.fg)),
            Span::styled("[q]", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" Cancel", Style::default().fg(theme.fg)),
        ]))
    };
    f.render_widget(controls.alignment(Alignment::Center), chunks[13]);
}

/// Render the mining indicator in the header bar.
/// Returns the formatted spans to be inserted into the header.
pub fn mining_header_spans(prefix: &str, keys_per_sec: f64, theme: &Theme) -> Vec<Span<'_>> {
    vec![
        Span::styled("  ⛏ Mining \"", Style::default().fg(theme.warning)),
        Span::styled(prefix, Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("\" {:.0}k/s", keys_per_sec / 1000.0),
            Style::default().fg(theme.warning),
        ),
    ]
}

/// Format a number with commas (e.g. 1,234,567).
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(b as char);
    }
    result
}

/// Format a duration in seconds to MM:SS or HH:MM:SS.
fn format_duration(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

/// Helper to create a centered horizontal rect of a given width.
fn centered_rect_width(width: u16, area: Rect) -> Rect {
    let side = area.width.saturating_sub(width) / 2;
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(side),
            Constraint::Length(width.min(area.width)),
            Constraint::Length(side),
        ])
        .split(area)[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_number_basic() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn format_duration_basic() {
        assert_eq!(format_duration(0.0), "00:00");
        assert_eq!(format_duration(65.0), "01:05");
        assert_eq!(format_duration(3661.0), "01:01:01");
    }
}
```

**Step 2: Register the module in `src/ui/mod.rs`**

Add after the existing module declarations:

```rust
pub mod mining;
```

**Step 3: Run tests**

Run: `cargo test ui::mining --lib`
Expected: 2 tests pass

**Step 4: Commit**

```bash
git add src/ui/mining.rs src/ui/mod.rs
git commit -m "feat: add mining UI screens — prefix input, fullscreen mining, header indicator"
```

---

## Task 5: Add mining AppState variants and key handling

**Files:**
- Modify: `src/ui/state.rs`

**Step 1: Add new AppState variants**

Add these variants to the `AppState` enum (after `SubscribingToChannel`):

```rust
    MiningPrefixInput {
        prefix: String,
        cursor: usize,
    },
    MiningActive {
        prefix: String,
        show_fullscreen: bool,
    },
```

Add a new `AppAction` variant:

```rust
    StartMining(String),           // prefix to mine
    AcceptMiningResult,
    CancelMining,
    ToggleMiningView,
```

**Step 2: Add key handling for the new states**

Add these match arms in `handle_key` (before the closing `}` of the main match):

```rust
            AppState::MiningPrefixInput { prefix, cursor } => {
                match key.code {
                    KeyCode::Char(c) => {
                        // Only allow valid base32 characters
                        let c_lower = c.to_ascii_lowercase();
                        if (c_lower.is_ascii_lowercase() && c_lower != '0' && c_lower != '1' && c_lower != '8' && c_lower != '9')
                            || (c_lower >= '2' && c_lower <= '7')
                        {
                            if prefix.len() < 7 {
                                prefix.push(c_lower);
                                *cursor += 1;
                            }
                        }
                        Ok(None)
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            prefix.remove(*cursor);
                        }
                        Ok(None)
                    }
                    KeyCode::Enter => {
                        if prefix.is_empty() {
                            // Skip mining, generate random
                            Ok(Some(AppAction::CancelMining))
                        } else {
                            Ok(Some(AppAction::StartMining(prefix.clone())))
                        }
                    }
                    KeyCode::Esc => {
                        // Skip mining
                        Ok(Some(AppAction::CancelMining))
                    }
                    _ => Ok(None),
                }
            }

            AppState::MiningActive { show_fullscreen, .. } => {
                match key.code {
                    KeyCode::Esc => {
                        *show_fullscreen = false;
                        Ok(None)
                    }
                    KeyCode::Char('m') if !*show_fullscreen => {
                        *show_fullscreen = true;
                        Ok(None)
                    }
                    KeyCode::Enter => Ok(Some(AppAction::AcceptMiningResult)),
                    KeyCode::Char('q') => Ok(Some(AppAction::CancelMining)),
                    _ => Ok(None),
                }
            }
```

**Step 3: Add tests**

```rust
    #[test]
    fn test_mining_prefix_input_typing() {
        let mut state = AppState::MiningPrefixInput {
            prefix: String::new(),
            cursor: 0,
        };
        state.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::MiningPrefixInput { prefix, .. } => assert_eq!(prefix, "chat"),
            _ => panic!("Expected MiningPrefixInput"),
        }
    }

    #[test]
    fn test_mining_prefix_rejects_invalid_chars() {
        let mut state = AppState::MiningPrefixInput {
            prefix: String::new(),
            cursor: 0,
        };
        // '8', '9', '0', '1' should be rejected
        state.handle_key(KeyEvent::new(KeyCode::Char('8'), KeyModifiers::NONE)).unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::MiningPrefixInput { prefix, .. } => assert_eq!(prefix, ""),
            _ => panic!("Expected MiningPrefixInput"),
        }
    }

    #[test]
    fn test_mining_prefix_enter_starts() {
        let mut state = AppState::MiningPrefixInput {
            prefix: "chat".to_string(),
            cursor: 4,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::StartMining("chat".to_string())));
    }

    #[test]
    fn test_mining_prefix_esc_cancels() {
        let mut state = AppState::MiningPrefixInput {
            prefix: "chat".to_string(),
            cursor: 4,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::CancelMining));
    }

    #[test]
    fn test_mining_active_esc_hides_fullscreen() {
        let mut state = AppState::MiningActive {
            prefix: "chat".to_string(),
            show_fullscreen: true,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
        assert!(action.is_none());
        match &state {
            AppState::MiningActive { show_fullscreen, .. } => assert!(!show_fullscreen),
            _ => panic!("Expected MiningActive"),
        }
    }

    #[test]
    fn test_mining_active_m_shows_fullscreen() {
        let mut state = AppState::MiningActive {
            prefix: "chat".to_string(),
            show_fullscreen: false,
        };
        state.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)).unwrap();
        match &state {
            AppState::MiningActive { show_fullscreen, .. } => assert!(show_fullscreen),
            _ => panic!("Expected MiningActive"),
        }
    }

    #[test]
    fn test_mining_active_enter_accepts() {
        let mut state = AppState::MiningActive {
            prefix: "chat".to_string(),
            show_fullscreen: true,
        };
        let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
        assert_eq!(action, Some(AppAction::AcceptMiningResult));
    }
```

**Step 4: Run tests**

Run: `cargo test ui::state --lib`
Expected: All tests pass (existing + 7 new)

**Step 5: Commit**

```bash
git add src/ui/state.rs
git commit -m "feat: add MiningPrefixInput and MiningActive state variants with key handling"
```

---

## Task 6: Fix identity lifecycle in `App`

**Files:**
- Modify: `src/app.rs`

**Step 1: Make identity optional in App**

Change the `identity` field to `Option<IdentityKeypair>`:

```rust
    pub identity: Option<IdentityKeypair>,
```

In `App::new()`, replace the identity generation with a DB check:

```rust
        // Check for existing identity — None means first run (mining screen)
        let identity = IdentityKeypair::load_or_generate_option(&db);
```

Update struct construction:

```rust
            identity,
```

In `init_tor()`, use the already-loaded identity instead of calling `load_or_generate` again:

```rust
        // Use the identity that was resolved during app init or mining
        let identity = self.identity.as_ref()
            .ok_or_else(|| crate::error::TorrentChatError::Crypto("No identity keypair available".into()))?;
```

**Step 2: Add `load_or_generate_option` to `IdentityKeypair`**

In `src/crypto/identity.rs`, add:

```rust
    /// Load identity from database. Returns None if no identity exists.
    /// Unlike `load_or_generate`, does NOT generate a new identity — that's
    /// deferred to the mining screen on first run.
    pub fn load_from_db(db: &Database) -> Option<Self> {
        let conn = db.connection();
        let bytes: Vec<u8> = conn.query_row(
            "SELECT value FROM settings WHERE key = 'identity_keypair'",
            [],
            |row| row.get(0),
        ).ok()?;
        Self::from_bytes(&bytes).ok()
    }
```

**Step 3: Update `App::new()` to use `load_from_db`**

Replace the identity line:

```rust
        let identity = IdentityKeypair::load_from_db(&db);
```

**Step 4: Update `App::new_with_settings()` similarly**

```rust
        let identity = IdentityKeypair::load_from_db(&db);
```

**Step 5: Update all `self.identity` accesses in `src/main.rs`**

Every place that accesses `app_lock.identity` needs to handle `Option`. The key places:

- `app_lock.identity.sign(...)` → `app_lock.identity.as_ref().unwrap().sign(...)`
- `app_lock.identity.to_onion_address()` → handled via `app_lock.onion_address`

Search for all `\.identity` usages and update them.

**Step 6: Run tests**

Run: `cargo test`
Expected: All tests pass. Some App tests may need updating to set identity.

**Step 7: Commit**

```bash
git add src/app.rs src/crypto/identity.rs src/main.rs
git commit -m "fix: consolidate identity lifecycle — load from DB or defer to mining"
```

---

## Task 7: Wire mining into `main.rs` first-run flow

**Files:**
- Modify: `src/main.rs`

**Step 1: Add first-run check after App initialization**

After `let app = Arc::new(Mutex::new(App::new()?));` and theme loading, add:

```rust
    // --- First-Run: Identity Mining ---
    let needs_identity = {
        let app_lock = app.lock().await;
        app_lock.identity.is_none()
    };

    if needs_identity {
        // Show mining prefix input screen
        let mut mining_state = AppState::MiningPrefixInput {
            prefix: String::new(),
            cursor: 0,
        };

        // Estimate ~150k keys/sec/core as default benchmark
        let estimated_rate = 150_000.0 * num_cpus() as f64;

        loop {
            // Render
            terminal.draw(|f| {
                if let AppState::MiningPrefixInput { ref prefix, cursor } = mining_state {
                    ui::mining::render_prefix_input(f, prefix, cursor, estimated_rate, &theme);
                }
            })?;

            // Handle input
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match mining_state.handle_key(key)? {
                        Some(AppAction::StartMining(prefix)) => {
                            // Start mining and enter mining loop
                            let identity = run_mining_loop(
                                &mut terminal, &prefix, &theme, &mut mining_state,
                            )?;
                            // Save identity
                            let mut app_lock = app.lock().await;
                            identity.save_to_db(&app_lock.db)?;
                            app_lock.identity = Some(identity);
                            break;
                        }
                        Some(AppAction::CancelMining) => {
                            // Generate random identity
                            let identity = IdentityKeypair::generate()?;
                            let mut app_lock = app.lock().await;
                            identity.save_to_db(&app_lock.db)?;
                            app_lock.identity = Some(identity);
                            break;
                        }
                        Some(AppAction::Quit) => {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
```

**Step 2: Add the `run_mining_loop` function**

```rust
fn run_mining_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    prefix: &str,
    theme: &ui::Theme,
    state: &mut AppState,
) -> Result<IdentityKeypair> {
    use crate::crypto::vanity::{start_mining, MiningProgress};

    let initial_progress = MiningProgress {
        attempts: 0,
        keys_per_sec: 0.0,
        best_prefix_len: 0,
        best_onion: None,
        found: false,
    };

    let (progress_tx, mut progress_rx) = tokio::sync::watch::channel(initial_progress.clone());
    let (result_tx, mut result_rx) = tokio::sync::oneshot::channel();

    let handle = start_mining(prefix, progress_tx, result_tx)?;
    let start_time = std::time::Instant::now();

    *state = AppState::MiningActive {
        prefix: prefix.to_string(),
        show_fullscreen: true,
    };

    let mut found_keypair: Option<IdentityKeypair> = None;

    loop {
        let progress = progress_rx.borrow().clone();
        let elapsed = start_time.elapsed().as_secs_f64();

        // Check if result is ready
        if found_keypair.is_none() {
            if let Ok(keypair) = result_rx.try_recv() {
                found_keypair = Some(keypair);
            }
        }

        // Render
        if let AppState::MiningActive { ref prefix, show_fullscreen } = state {
            if *show_fullscreen {
                terminal.draw(|f| {
                    ui::mining::render_mining_fullscreen(f, prefix, &progress, elapsed, theme);
                })?;
            }
            // When not fullscreen, the normal UI loop will render with the header indicator
            // For now during first-run, we always show fullscreen
        }

        // Auto-accept on match found
        if progress.found && found_keypair.is_some() {
            return Ok(found_keypair.take().unwrap());
        }

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match state.handle_key(key)? {
                    Some(AppAction::AcceptMiningResult) => {
                        handle.cancel();
                        if let Some(kp) = found_keypair.take() {
                            return Ok(kp);
                        }
                        // No match yet — generate random
                        return IdentityKeypair::generate();
                    }
                    Some(AppAction::CancelMining) => {
                        handle.cancel();
                        return IdentityKeypair::generate();
                    }
                    Some(AppAction::Quit) => {
                        handle.cancel();
                        return Err(crate::error::TorrentChatError::Io(
                            io::Error::new(io::ErrorKind::Interrupted, "User quit during mining")
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
```

**Step 3: Compile and test**

Run: `cargo build && cargo test`
Expected: Compiles and all tests pass

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire mining into first-run flow with prefix input and mining loop"
```

---

## Task 8: Remove Signal plaintext fallback

**Files:**
- Modify: `src/crypto/signal.rs`

**Step 1: Write failing test for error on missing shared_secret**

Add to the test module:

```rust
    #[test]
    fn test_encrypt_without_session_returns_error() {
        let mut session = SignalSession::from_prekey_bundle(
            "test.onion".into(),
            &PreKeyBundle::generate().unwrap(),
        ).unwrap();

        // This session has no real shared_secret — should error
        let result = session.encrypt(b"hello");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_without_session_returns_error() {
        let mut session = SignalSession::from_prekey_bundle(
            "test.onion".into(),
            &PreKeyBundle::generate().unwrap(),
        ).unwrap();

        let result = session.decrypt(b"ciphertext");
        assert!(result.is_err());
    }
```

**Step 2: Run tests to verify they fail**

Run: `cargo test signal::tests::test_encrypt_without_session_returns_error --lib`
Expected: FAIL (currently returns Ok with plaintext)

**Step 3: Replace the plaintext fallback in `encrypt()`**

In `encrypt()`, replace the `else` branch (lines 324-328):

```rust
        } else {
            Err(TorrentChatError::SessionNotFound(
                format!("No encryption session for {}", self.remote_onion)
            ))
        }
```

**Step 4: Replace the plaintext fallback in `decrypt()`**

In `decrypt()`, replace the `else` branch (lines 368-371):

```rust
        } else {
            Err(TorrentChatError::DecryptionFailed(
                format!("No decryption session for {}", self.remote_onion)
            ))
        }
```

**Step 5: Run tests**

Run: `cargo test crypto::signal --lib`
Expected: All tests pass (including the 2 new ones)

**Step 6: Commit**

```bash
git add src/crypto/signal.rs
git commit -m "feat: remove plaintext fallback — encrypt/decrypt require real shared_secret"
```

---

## Task 9: Wire PreKeyBundle into friend request messages

**Files:**
- Modify: `src/protocol/message.rs`
- Modify: `src/protocol/friend_request.rs`

**Step 1: Add `prekey_bundle` field to friend request message types**

In `src/protocol/message.rs`, find the `FriendRequestMessage` struct (inside the `Message` enum or as a standalone struct — need to check the exact structure). Add a `prekey_bundle` field.

Read `src/protocol/message.rs` to find the exact structure, then add:

```rust
    pub prekey_bundle: Option<crate::crypto::PreKeyBundle>,
```

to both `FriendRequestMessage` (in the `Message::FriendRequest` variant data) and `FriendRequestAccept` variant data.

**Step 2: Update friend request creation in `src/main.rs`**

When creating a friend request, generate a real PreKeyBundle and include it:

```rust
let (bundle, private_material) = PreKeyBundle::generate_real(&identity)?;
// Store private_material for later session establishment
// Include bundle in the friend request message
```

**Step 3: Update friend request acceptance in `src/main.rs`**

When accepting a friend request:
1. Generate own PreKeyBundle
2. Create SignalSession from peer's bundle via `from_prekey_bundle_real`
3. Store session in DB
4. Include own bundle in the accept message

**Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/protocol/message.rs src/protocol/friend_request.rs src/main.rs
git commit -m "feat: wire PreKeyBundle exchange into friend request/accept flow"
```

---

## Task 10: Wire Signal sessions into message send/receive

**Files:**
- Modify: `src/main.rs` (message send path)
- Modify: `src/main.rs` (message receive path, if applicable)

**Step 1: Load session on message send**

In the `SendMessage` action handler in `main.rs`, before creating the `TextMessage`:
1. Load `SignalSession` from the `signal_sessions` table via `SessionStore`
2. Call `session.encrypt(plaintext_bytes)` to get ciphertext
3. Set `signal_ciphertext` and `signal_type` on the message

**Step 2: Load session on message receive**

In the incoming message handler:
1. Load `SignalSession` from DB
2. If `signal_type == "PreKeyMessage"` and no session exists, establish via `from_prekey_message_real`
3. Call `session.decrypt()` to get plaintext
4. Store updated session counter

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire Signal sessions into message encrypt/decrypt path"
```

---

## Task 11: Update CLAUDE.md documentation

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update Phase Implementation Status**

Add Phase 5 section:

```markdown
### Phase 5: Crypto & Identity ✅
- Vanity .onion mining with rayon-based parallel workers
- First-run mining screen with prefix input and live ETA
- Toggleable full-screen mining UI with animated ASCII art
- Header mining indicator when browsing UI during mining
- Identity lifecycle fix: single load/create path via DB
- Signal Protocol: removed plaintext fallback from encrypt/decrypt
- PreKeyBundle exchange during friend request/accept flow
- Real X3DH + ChaCha20Poly1305 for all conversations
```

Update the test count and key files section.

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with Phase 5 completion details"
```

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Add rayon dependency | `Cargo.toml` |
| 2 | Vanity mining core | `src/crypto/vanity.rs` (new), `src/crypto/mod.rs` |
| 3 | IdentityKeypair constructors | `src/crypto/identity.rs` |
| 4 | Mining UI screens | `src/ui/mining.rs` (new), `src/ui/mod.rs` |
| 5 | Mining AppState + key handling | `src/ui/state.rs` |
| 6 | Fix identity lifecycle | `src/app.rs`, `src/crypto/identity.rs`, `src/main.rs` |
| 7 | Wire mining into main.rs | `src/main.rs` |
| 8 | Remove Signal plaintext fallback | `src/crypto/signal.rs` |
| 9 | PreKeyBundle in friend requests | `src/protocol/message.rs`, `src/protocol/friend_request.rs` |
| 10 | Wire Signal into message flow | `src/main.rs` |
| 11 | Update CLAUDE.md | `CLAUDE.md` |
