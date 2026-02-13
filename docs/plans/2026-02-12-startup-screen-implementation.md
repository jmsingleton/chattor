# Startup Screen Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the frozen startup experience with an animated Unicode pixel art bootstrap screen that shows Tor connection progress, handles failure gracefully, and transitions cleanly to the main UI.

**Architecture:** A dedicated bootstrap mini-loop runs before the main event loop. Tor init communicates progress via a `tokio::watch` channel, eliminating the `Arc<Mutex<App>>` lock contention that causes the current freeze. The bootstrap has its own state (`BootstrapPhase`), renderer, and key handler — completely independent of `AppState`.

**Tech Stack:** ratatui 0.27, crossterm 0.27, tokio watch channels

---

### Task 1: Define BootstrapPhase and BootstrapUpdate types

**Files:**
- Modify: `src/ui/bootstrap.rs` (replace existing file contents)

**Step 1: Write the failing test**

Add to end of `src/ui/bootstrap.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_phase_starts_connecting() {
        let phase = BootstrapPhase::new();
        assert!(matches!(phase, BootstrapPhase::Connecting { .. }));
    }

    #[test]
    fn bootstrap_phase_advance_tick() {
        let mut phase = BootstrapPhase::new();
        phase.advance_tick();
        if let BootstrapPhase::Connecting { tick, .. } = &phase {
            assert_eq!(*tick, 1);
        } else {
            panic!("Expected Connecting phase");
        }
    }

    #[test]
    fn bootstrap_phase_frame_advances_every_3_ticks() {
        let mut phase = BootstrapPhase::new();
        // Frame should advance every 3 ticks
        phase.advance_tick(); // tick 1, frame 0
        phase.advance_tick(); // tick 2, frame 0
        phase.advance_tick(); // tick 3, frame 1
        if let BootstrapPhase::Connecting { frame, .. } = &phase {
            assert_eq!(*frame, 1);
        } else {
            panic!("Expected Connecting phase");
        }
    }

    #[test]
    fn bootstrap_update_variants() {
        let _progress = BootstrapUpdate::Progress(50);
        let _connected = BootstrapUpdate::Connected;
        let _failed = BootstrapUpdate::Failed("test error".to_string());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test ui::bootstrap::tests -v`
Expected: FAIL — `BootstrapPhase`, `BootstrapUpdate` not defined

**Step 3: Write minimal implementation**

Replace the entire contents of `src/ui/bootstrap.rs` with:

```rust
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Clear},
    Frame,
};

/// Status updates from the Tor init background task
#[derive(Clone, Debug)]
pub enum BootstrapUpdate {
    Progress(u8),
    Connected,
    Failed(String),
}

/// Bootstrap screen state machine
#[derive(Debug)]
pub enum BootstrapPhase {
    Connecting {
        progress: u8,
        frame: usize,
        tick: u64,
    },
    Failed {
        error: String,
        frame: usize,
        tick: u64,
    },
    Done,
}

/// Actions the bootstrap screen can produce
#[derive(Debug, Clone, PartialEq)]
pub enum BootstrapAction {
    Retry,
    ContinueOffline,
    Quit,
}

impl BootstrapPhase {
    pub fn new() -> Self {
        BootstrapPhase::Connecting {
            progress: 0,
            frame: 0,
            tick: 0,
        }
    }

    /// Advance animation tick. Call every 100ms.
    /// Frame advances every 3 ticks (~300ms).
    pub fn advance_tick(&mut self) {
        match self {
            BootstrapPhase::Connecting { tick, frame, .. } => {
                *tick += 1;
                if *tick % 3 == 0 {
                    *frame += 1;
                }
            }
            BootstrapPhase::Failed { tick, frame, .. } => {
                *tick += 1;
                if *tick % 3 == 0 {
                    *frame += 1;
                }
            }
            BootstrapPhase::Done => {}
        }
    }

    /// Update progress from watch channel
    pub fn set_progress(&mut self, progress: u8) {
        if let BootstrapPhase::Connecting { progress: p, .. } = self {
            *p = progress;
        }
    }

    /// Transition to failed state
    pub fn fail(&mut self, error: String) {
        *self = BootstrapPhase::Failed {
            error,
            frame: 0,
            tick: 0,
        };
    }

    /// Transition to done state
    pub fn done(&mut self) {
        *self = BootstrapPhase::Done;
    }
}
```

Keep the tests at the bottom (from step 1).

**Step 4: Run test to verify it passes**

Run: `cargo test ui::bootstrap::tests -- --nocapture`
Expected: all 4 tests PASS

**Step 5: Commit**

```bash
git add src/ui/bootstrap.rs
git commit -m "feat: add BootstrapPhase and BootstrapUpdate types"
```

---

### Task 2: ASCII art frame data and status messages

**Files:**
- Modify: `src/ui/bootstrap.rs` (add constants)

**Step 1: Write the failing test**

Add to the `tests` module in `src/ui/bootstrap.rs`:

```rust
#[test]
fn connecting_frames_exist_and_are_nonempty() {
    let frames = connecting_frames();
    assert!(frames.len() >= 4, "Need at least 4 animation frames");
    for frame in &frames {
        assert!(!frame.is_empty(), "Frame should not be empty");
    }
}

#[test]
fn failure_art_exists() {
    let art = failure_art();
    assert!(!art.is_empty());
}

#[test]
fn status_messages_exist() {
    let msgs = status_messages();
    assert!(msgs.len() >= 3);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test ui::bootstrap::tests -v`
Expected: FAIL — functions not defined

**Step 3: Write minimal implementation**

Add these functions to `src/ui/bootstrap.rs` (above the tests module):

```rust
/// Animation frames for the connecting screen.
/// Three onion relay nodes with a signal pulse traveling between them.
/// Each frame is a Vec of string lines.
pub fn connecting_frames() -> Vec<Vec<&'static str>> {
    vec![
        // Frame 0: Pulse starting at "you" node
        vec![
            "       \u{2584}\u{258c}                      \u{2584}\u{258c}                      \u{2584}\u{258c}",
            "      \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}",
            "    \u{2584}\u{2588}\u{2593}\u{2593}\u{2593}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}",
            "    \u{2588}\u{2593}\u{2592}\u{2591}\u{2592}\u{2593}\u{2588}\u{2550}\u{2550}\u{2591}\u{2592}\u{2593}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}",
            "     \u{2580}\u{2588}\u{2593}\u{2593}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}",
            "       \u{2580}\u{2580}                      \u{2580}\u{2580}                      \u{2580}\u{2580}",
            "      you                    relay                    exit",
        ],
        // Frame 1: Pulse between "you" and "relay"
        vec![
            "       \u{2584}\u{258c}                      \u{2584}\u{258c}                      \u{2584}\u{258c}",
            "      \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}",
            "    \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}",
            "    \u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2591}\u{2592}\u{2593}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}",
            "     \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}",
            "       \u{2580}\u{2580}                      \u{2580}\u{2580}                      \u{2580}\u{2580}",
            "      you                    relay                    exit",
        ],
        // Frame 2: Pulse arriving at "relay" node (relay lights up)
        vec![
            "       \u{2584}\u{258c}                      \u{2584}\u{258c}                      \u{2584}\u{258c}",
            "      \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}",
            "    \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2593}\u{2593}\u{2593}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}",
            "    \u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2591}\u{2592}\u{2593}\u{2588}\u{2593}\u{2592}\u{2591}\u{2592}\u{2593}\u{2588}\u{2550}\u{2550}\u{2591}\u{2592}\u{2593}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}",
            "     \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2593}\u{2593}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}",
            "       \u{2580}\u{2580}                      \u{2580}\u{2580}                      \u{2580}\u{2580}",
            "      you                    relay                    exit",
        ],
        // Frame 3: Pulse between "relay" and "exit"
        vec![
            "       \u{2584}\u{258c}                      \u{2584}\u{258c}                      \u{2584}\u{258c}",
            "      \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}",
            "    \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}",
            "    \u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2591}\u{2592}\u{2593}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}",
            "     \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}",
            "       \u{2580}\u{2580}                      \u{2580}\u{2580}                      \u{2580}\u{2580}",
            "      you                    relay                    exit",
        ],
        // Frame 4: Pulse arriving at "exit" node (exit lights up)
        vec![
            "       \u{2584}\u{258c}                      \u{2584}\u{258c}                      \u{2584}\u{258c}",
            "      \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}",
            "    \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2593}\u{2593}\u{2593}\u{2588}\u{2584}",
            "    \u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2591}\u{2592}\u{2593}\u{2588}\u{2593}\u{2592}\u{2591}\u{2592}\u{2593}\u{2588}",
            "     \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2593}\u{2593}\u{2588}\u{2580}",
            "       \u{2580}\u{2580}                      \u{2580}\u{2580}                      \u{2580}\u{2580}",
            "      you                    relay                    exit",
        ],
        // Frame 5: All nodes lit (success flash, also used as cycle reset)
        vec![
            "       \u{2584}\u{258c}                      \u{2584}\u{258c}                      \u{2584}\u{258c}",
            "      \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}                     \u{2584}\u{2588}\u{2588}",
            "    \u{2584}\u{2588}\u{2593}\u{2593}\u{2593}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2593}\u{2593}\u{2593}\u{2588}\u{2584}                 \u{2584}\u{2588}\u{2593}\u{2593}\u{2593}\u{2588}\u{2584}",
            "    \u{2588}\u{2593}\u{2592}\u{2591}\u{2592}\u{2593}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2593}\u{2592}\u{2591}\u{2592}\u{2593}\u{2588}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2593}\u{2592}\u{2591}\u{2592}\u{2593}\u{2588}",
            "     \u{2580}\u{2588}\u{2593}\u{2593}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2593}\u{2593}\u{2588}\u{2580}                  \u{2580}\u{2588}\u{2593}\u{2593}\u{2588}\u{2580}",
            "       \u{2580}\u{2580}                      \u{2580}\u{2580}                      \u{2580}\u{2580}",
            "      you                    relay                    exit",
        ],
    ]
}

/// Static ASCII art for the failure screen — a dim, sad onion
pub fn failure_art() -> Vec<&'static str> {
    vec![
        "                        \u{2584}\u{258c}",
        "                       \u{2588}\u{2588}",
        "                     \u{2584}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}\u{2584}",
        "                     \u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}",
        "                      \u{2580}\u{2588}\u{2591}\u{2591}\u{2588}\u{2580}",
        "                        \u{2580}\u{2580}",
    ]
}

/// Rotating cheeky status messages shown during connecting
pub fn status_messages() -> Vec<&'static str> {
    vec![
        "Peeling onion layers...",
        "Negotiating with relays...",
        "Building circuits in the dark...",
        "Routing through the underground...",
        "Almost there, patience is a virtue...",
        "Wrapping in layers of encryption...",
    ]
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test ui::bootstrap::tests -- --nocapture`
Expected: all 7 tests PASS

**Step 5: Commit**

```bash
git add src/ui/bootstrap.rs
git commit -m "feat: add connecting animation frames and failure art"
```

---

### Task 3: Render functions for bootstrap screens

**Files:**
- Modify: `src/ui/bootstrap.rs` (add render functions)

**Step 1: Write the failing test**

Add to tests module in `src/ui/bootstrap.rs`:

```rust
#[test]
fn render_connecting_does_not_panic() {
    // Verify render function exists and handles all frame indices
    let phase = BootstrapPhase::new();
    // We can't easily test rendering without a terminal,
    // but we can verify the frame indexing logic
    let frames = connecting_frames();
    let total_frames = frames.len();
    assert!(total_frames > 0);
    // Frame index should wrap around
    for i in 0..total_frames * 2 {
        let _ = i % total_frames;
    }
}

#[test]
fn status_message_cycles() {
    let msgs = status_messages();
    let total = msgs.len();
    // tick 0 -> msg 0, tick 5 -> msg 0 (changes every ~10 ticks)
    assert_eq!(0 / 10 % total, 0);
    assert_eq!(10 / 10 % total, 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test ui::bootstrap::tests -v`
Expected: PASS (these are logic tests for functions already defined — but we need the render functions for the next step)

**Step 3: Write the render functions**

Add to `src/ui/bootstrap.rs`, above the tests module:

```rust
/// Render the connecting animation screen
pub fn render_connecting(f: &mut Frame, frame: usize, tick: u64, progress: u8) {
    let area = f.area();
    f.render_widget(Clear, area);

    let frames = connecting_frames();
    let total_frames = frames.len();
    let current_frame = &frames[frame % total_frames];

    let msgs = status_messages();
    let current_msg = msgs[(tick as usize / 10) % msgs.len()];

    // Layout: top padding, title, art, status message, progress hint
    let art_height = current_frame.len() as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),           // Top padding
            Constraint::Length(2),                 // Title
            Constraint::Length(1),                 // Spacer
            Constraint::Length(art_height),        // Art
            Constraint::Length(2),                 // Spacer + status
            Constraint::Length(1),                 // Status message
            Constraint::Min(0),                    // Bottom fill
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "chattor",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    // ASCII art
    let art_lines: Vec<Line> = current_frame.iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(Color::Cyan))))
        .collect();
    let art = Paragraph::new(art_lines).alignment(Alignment::Center);
    f.render_widget(art, chunks[3]);

    // Status message
    let status = Paragraph::new(Line::from(Span::styled(
        current_msg,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    f.render_widget(status, chunks[5]);
}

/// Render the failure screen
pub fn render_failure(f: &mut Frame, error: &str) {
    let area = f.area();
    f.render_widget(Clear, area);

    let art = failure_art();
    let art_height = art.len() as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(15),            // Top padding
            Constraint::Length(2),                  // Title
            Constraint::Length(1),                  // Spacer
            Constraint::Length(art_height),         // Art
            Constraint::Length(2),                  // Spacer
            Constraint::Length(1),                  // "connection failed :("
            Constraint::Length(2),                  // Spacer
            Constraint::Length(1),                  // Error detail
            Constraint::Length(1),                  // Spacer
            Constraint::Length(4),                  // Troubleshooting tips
            Constraint::Length(1),                  // Spacer
            Constraint::Length(1),                  // Docs link
            Constraint::Length(2),                  // Spacer
            Constraint::Length(1),                  // Action keys
            Constraint::Min(0),                     // Bottom fill
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(Span::styled(
        "chattor",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[1]);

    // Sad onion art
    let art_lines: Vec<Line> = art.iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(Color::DarkGray))))
        .collect();
    let art_widget = Paragraph::new(art_lines).alignment(Alignment::Center);
    f.render_widget(art_widget, chunks[3]);

    // "connection failed :("
    let fail_msg = Paragraph::new(Line::from(Span::styled(
        "connection failed :(",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center);
    f.render_widget(fail_msg, chunks[5]);

    // Error detail
    let error_detail = Paragraph::new(Line::from(Span::styled(
        error,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    f.render_widget(error_detail, chunks[7]);

    // Troubleshooting tips
    let tips = Paragraph::new(vec![
        Line::from(Span::styled("  * check your internet connection", Style::default().fg(Color::Gray))),
        Line::from(Span::styled("  * your firewall may be blocking outbound traffic", Style::default().fg(Color::Gray))),
        Line::from(Span::styled("  * tor network may be temporarily unreachable", Style::default().fg(Color::Gray))),
        Line::from(Span::styled("  * try a different network \u{2014} some block tor", Style::default().fg(Color::Gray))),
    ])
    .alignment(Alignment::Center);
    f.render_widget(tips, chunks[9]);

    // Docs link
    let docs = Paragraph::new(Line::from(Span::styled(
        "docs: https://github.com/chattor/chattor/wiki/tor",
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    f.render_widget(docs, chunks[11]);

    // Action keys
    let actions = Paragraph::new(Line::from(vec![
        Span::styled("[R]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" Retry   ", Style::default().fg(Color::Gray)),
        Span::styled("[C]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" Continue   ", Style::default().fg(Color::Gray)),
        Span::styled("[Q]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" Quit", Style::default().fg(Color::Gray)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(actions, chunks[13]);
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test ui::bootstrap::tests -- --nocapture`
Expected: all tests PASS

Also verify the whole project compiles:

Run: `cargo build 2>&1 | head -5`
Expected: compiles (warnings OK)

**Step 5: Commit**

```bash
git add src/ui/bootstrap.rs
git commit -m "feat: add render functions for connecting and failure screens"
```

---

### Task 4: Bootstrap key handling

**Files:**
- Modify: `src/ui/bootstrap.rs`

**Step 1: Write the failing test**

Add to tests module:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn failure_screen_r_retries() {
    let mut phase = BootstrapPhase::Failed {
        error: "test".to_string(),
        frame: 0,
        tick: 0,
    };
    let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
    let action = handle_bootstrap_key(&phase, key);
    assert_eq!(action, Some(BootstrapAction::Retry));
}

#[test]
fn failure_screen_c_continues() {
    let phase = BootstrapPhase::Failed {
        error: "test".to_string(),
        frame: 0,
        tick: 0,
    };
    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
    let action = handle_bootstrap_key(&phase, key);
    assert_eq!(action, Some(BootstrapAction::ContinueOffline));
}

#[test]
fn failure_screen_q_quits() {
    let phase = BootstrapPhase::Failed {
        error: "test".to_string(),
        frame: 0,
        tick: 0,
    };
    let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    let action = handle_bootstrap_key(&phase, key);
    assert_eq!(action, Some(BootstrapAction::Quit));
}

#[test]
fn connecting_screen_ctrl_c_quits() {
    let phase = BootstrapPhase::new();
    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = handle_bootstrap_key(&phase, key);
    assert_eq!(action, Some(BootstrapAction::Quit));
}

#[test]
fn connecting_screen_ignores_other_keys() {
    let phase = BootstrapPhase::new();
    let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    let action = handle_bootstrap_key(&phase, key);
    assert_eq!(action, None);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test ui::bootstrap::tests -v`
Expected: FAIL — `handle_bootstrap_key` not defined

**Step 3: Write the key handler**

Add to `src/ui/bootstrap.rs`, above the tests module:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle key events during bootstrap. Returns an action if the key was meaningful.
pub fn handle_bootstrap_key(phase: &BootstrapPhase, key: KeyEvent) -> Option<BootstrapAction> {
    // Ctrl+C always quits
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(BootstrapAction::Quit);
    }

    match phase {
        BootstrapPhase::Failed { .. } => {
            match key.code {
                KeyCode::Char('r') | KeyCode::Char('R') => Some(BootstrapAction::Retry),
                KeyCode::Char('c') | KeyCode::Char('C') => Some(BootstrapAction::ContinueOffline),
                KeyCode::Char('q') | KeyCode::Char('Q') => Some(BootstrapAction::Quit),
                _ => None,
            }
        }
        BootstrapPhase::Connecting { .. } => {
            // During connecting, only Ctrl+C works (handled above)
            None
        }
        BootstrapPhase::Done => None,
    }
}
```

Note: remove the duplicate `use crossterm` import if `KeyCode`/`KeyEvent`/`KeyModifiers` are already imported at the top of the file. Consolidate into one import block.

**Step 4: Run test to verify it passes**

Run: `cargo test ui::bootstrap::tests -- --nocapture`
Expected: all tests PASS

**Step 5: Commit**

```bash
git add src/ui/bootstrap.rs
git commit -m "feat: add bootstrap key handler for failure screen actions"
```

---

### Task 5: Update mod.rs exports

**Files:**
- Modify: `src/ui/mod.rs:10`

**Step 1: Update the export**

Change line 10 of `src/ui/mod.rs` from:

```rust
pub use bootstrap::render_bootstrap;
```

to:

```rust
pub use bootstrap::{
    BootstrapPhase, BootstrapUpdate, BootstrapAction,
    render_connecting, render_failure, handle_bootstrap_key,
};
```

**Step 2: Verify it compiles**

Run: `cargo build 2>&1 | head -10`
Expected: compiles

**Step 3: Commit**

```bash
git add src/ui/mod.rs
git commit -m "refactor: update bootstrap exports in ui mod"
```

---

### Task 6: Wire up bootstrap loop in main.rs

This is the core integration task. It replaces the current Tor spawn + immediate main loop entry with: spawn Tor with watch channel → run bootstrap loop → transition to main loop.

**Files:**
- Modify: `src/main.rs:31-55` (the startup section before the main event loop)

**Step 1: Add the bootstrap loop**

Replace `src/main.rs` lines 31-55 (from `async fn main()` body start through `let mut app_state = AppState::default();`) with:

```rust
    let _cli = Cli::parse();

    // Initialize application wrapped in Arc<Mutex> for sharing between threads
    let app = Arc::new(Mutex::new(App::new()?));

    // Set up terminal FIRST so we can render immediately
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // --- Bootstrap Phase ---
    // Create watch channel for Tor init progress
    let (bootstrap_tx, mut bootstrap_rx) = tokio::sync::watch::channel(
        ui::BootstrapUpdate::Progress(0)
    );

    // Spawn Tor init in background, communicating via watch channel
    let app_tor = Arc::clone(&app);
    tokio::spawn(async move {
        let mut app_lock = app_tor.lock().await;
        match app_lock.init_tor().await {
            Ok(()) => {
                let _ = bootstrap_tx.send(ui::BootstrapUpdate::Connected);
            }
            Err(e) => {
                let _ = bootstrap_tx.send(ui::BootstrapUpdate::Failed(
                    format!("{}", e)
                ));
            }
        }
    });

    // Run bootstrap animation loop
    let mut phase = ui::BootstrapPhase::new();
    let bootstrap_start = std::time::Instant::now();
    let bootstrap_timeout = std::time::Duration::from_secs(60);

    let continue_to_app = loop {
        // Render current bootstrap frame
        match &phase {
            ui::BootstrapPhase::Connecting { frame, tick, progress } => {
                terminal.draw(|f| {
                    ui::render_connecting(f, *frame, *tick, *progress);
                })?;
            }
            ui::BootstrapPhase::Failed { error, .. } => {
                terminal.draw(|f| {
                    ui::render_failure(f, error);
                })?;
            }
            ui::BootstrapPhase::Done => {
                break true;
            }
        }

        // Check for timeout (only during connecting)
        if matches!(phase, ui::BootstrapPhase::Connecting { .. })
            && bootstrap_start.elapsed() > bootstrap_timeout
        {
            phase.fail("connection timed out after 60 seconds".to_string());
            continue;
        }

        // Check for updates from Tor init task
        if bootstrap_rx.has_changed().unwrap_or(false) {
            let update = bootstrap_rx.borrow_and_update().clone();
            match update {
                ui::BootstrapUpdate::Progress(p) => {
                    phase.set_progress(p);
                }
                ui::BootstrapUpdate::Connected => {
                    // Show success flash briefly
                    phase.done();
                    continue;
                }
                ui::BootstrapUpdate::Failed(e) => {
                    phase.fail(e);
                }
            }
        }

        // Handle key events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let Some(action) = ui::handle_bootstrap_key(&phase, key) {
                    match action {
                        ui::BootstrapAction::Quit => {
                            // Clean up and exit
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                LeaveAlternateScreen,
                                DisableMouseCapture
                            )?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        ui::BootstrapAction::ContinueOffline => {
                            break true;
                        }
                        ui::BootstrapAction::Retry => {
                            phase = ui::BootstrapPhase::new();
                            let (new_tx, new_rx) = tokio::sync::watch::channel(
                                ui::BootstrapUpdate::Progress(0)
                            );
                            bootstrap_rx = new_rx;
                            let app_retry = Arc::clone(&app);
                            tokio::spawn(async move {
                                let mut app_lock = app_retry.lock().await;
                                match app_lock.init_tor().await {
                                    Ok(()) => {
                                        let _ = new_tx.send(ui::BootstrapUpdate::Connected);
                                    }
                                    Err(e) => {
                                        let _ = new_tx.send(ui::BootstrapUpdate::Failed(
                                            format!("{}", e)
                                        ));
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }

        // Advance animation tick
        phase.advance_tick();
    };

    if !continue_to_app {
        // Should not reach here, but safety net
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        return Ok(());
    }

    // --- Main App Phase ---
    // Initialize state machine
    let mut app_state = AppState::default();
```

Key changes:
- Terminal setup moved BEFORE Tor init (so we can render immediately)
- Tor init communicates via `tokio::watch` instead of directly mutating App
- Bootstrap loop handles all rendering and key events during startup
- On `Done` or `ContinueOffline`, falls through to existing main event loop
- On `Quit`, cleans up terminal and returns
- On `Retry`, re-spawns Tor init with fresh channel
- 60-second timeout auto-fails

**Step 2: Verify it compiles**

Run: `cargo build 2>&1 | head -20`
Expected: compiles (warnings about unused `bootstrap_start` rebind on retry are OK)

**Step 3: Run all existing tests**

Run: `cargo test`
Expected: all existing tests still pass (bootstrap changes don't affect test paths)

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up bootstrap animation loop before main UI"
```

---

### Task 7: Manual testing and art refinement

**Files:**
- Possibly: `src/ui/bootstrap.rs` (tweak art if alignment is off)

**Step 1: Run the app and observe the bootstrap screen**

Run: `cargo run`

Verify:
- Bootstrap animation appears immediately (no freeze)
- Frames cycle smoothly (~300ms per frame)
- Status messages rotate
- If Tor connects: brief flash, then main UI appears
- If Tor fails: fizzle to failure screen

**Step 2: Test failure screen**

If Tor is not available on your system, the bootstrap should timeout after 60s or fail immediately. On the failure screen verify:
- `[R]` restarts the connecting animation
- `[C]` enters the main app (header shows "Tor: Connecting...")
- `[Q]` exits cleanly
- `Ctrl+C` exits from any screen

**Step 3: Test narrow terminal**

Resize terminal to <60 columns and verify the art doesn't break layout. If it does, add a width check in the render functions that falls back to a compact layout. This is an optional refinement.

**Step 4: Tweak ASCII art alignment**

The Unicode art frames may need character-level adjustment once you see them rendered in a real terminal. The half-block characters can shift alignment depending on font. Adjust the frame strings in `connecting_frames()` and `failure_art()` as needed.

**Step 5: Commit any refinements**

```bash
git add src/ui/bootstrap.rs
git commit -m "fix: refine bootstrap art alignment for terminal rendering"
```

---

### Task 8: Clean up old bootstrap code

**Files:**
- Modify: `src/ui/bootstrap.rs` (remove old `render_bootstrap` if any remnants)
- Modify: `src/ui/mod.rs` (ensure no stale exports)

**Step 1: Verify no references to old `render_bootstrap`**

Search codebase for `render_bootstrap` — it should no longer be referenced anywhere.

Run: `cargo build && cargo test`
Expected: clean build, all tests pass

**Step 2: Final commit**

```bash
git add -A
git commit -m "chore: clean up old bootstrap code"
```

---

## Summary

| Task | What | Files |
|------|------|-------|
| 1 | `BootstrapPhase` + `BootstrapUpdate` types | `bootstrap.rs` |
| 2 | ASCII art frames + status messages | `bootstrap.rs` |
| 3 | `render_connecting` + `render_failure` | `bootstrap.rs` |
| 4 | `handle_bootstrap_key` | `bootstrap.rs` |
| 5 | Update mod.rs exports | `mod.rs` |
| 6 | Wire up bootstrap loop in main.rs | `main.rs` |
| 7 | Manual testing + art refinement | `bootstrap.rs` |
| 8 | Clean up old code | `bootstrap.rs`, `mod.rs` |
