# Phase 4: Polish & Theming Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform chattor's TUI into a visually polished application with a full theming system, 7 preset themes, TOML config overrides, and improved UX for identity sharing and clipboard.

**Architecture:** Theme struct with compiled-in presets + optional TOML overrides. Every UI component receives `&Theme` and reads colors from it instead of hardcoding them. CLI `--theme` flag for quick preset switching.

**Tech Stack:** ratatui (TUI), serde/toml (config parsing — both already in Cargo.toml), arboard (clipboard — already in Cargo.toml), clap (CLI args)

---

## Task 1: Create Theme struct and hex color parsing

**Files:**
- Create: `src/ui/theme.rs`
- Modify: `src/ui/mod.rs`

**Step 1: Create `src/ui/theme.rs` with Theme struct, `parse_hex_color`, and tests**

```rust
use ratatui::style::Color;

/// All colors used across the UI, grouped by component.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,

    // Global
    pub bg: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub accent: Color,
    pub border: Color,
    pub border_focused: Color,

    // Header
    pub header_fg: Color,
    pub header_accent: Color,

    // Sidebar
    pub sidebar_selected_fg: Color,
    pub sidebar_unread: Color,
    pub sidebar_status_online: Color,
    pub sidebar_channel_header: Color,

    // Conversation
    pub msg_own_sender: Color,
    pub msg_peer_sender: Color,
    pub msg_timestamp: Color,
    pub msg_status_sent: Color,
    pub msg_status_delivered: Color,
    pub msg_status_read: Color,
    pub msg_status_failed: Color,
    pub msg_ephemeral: Color,

    // Input
    pub input_fg: Color,
    pub input_placeholder: Color,

    // Modals
    pub modal_border: Color,
    pub modal_title: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,

    // Channel feed
    pub channel_border: Color,
    pub channel_read_count: Color,
}

/// Parse a hex color string like "#00d4ff" into a ratatui Color::Rgb.
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Shorthand: parse hex or panic (for preset definitions only).
fn h(hex: &str) -> Color {
    parse_hex_color(hex).unwrap_or(Color::White)
}

impl Theme {
    /// Load a preset theme by name. Falls back to "dark" for unknown names.
    pub fn preset(name: &str) -> Self {
        match name {
            "dark" => Self::dark(),
            "light" => Self::light(),
            "cyberpunk" => Self::cyberpunk(),
            "minimal" => Self::minimal(),
            "rose-pine" => Self::rose_pine(),
            "rose-pine-moon" => Self::rose_pine_moon(),
            "rose-pine-dawn" => Self::rose_pine_dawn(),
            _ => Self::dark(),
        }
    }

    /// List all available preset names.
    pub fn preset_names() -> &'static [&'static str] {
        &["dark", "light", "cyberpunk", "minimal", "rose-pine", "rose-pine-moon", "rose-pine-dawn"]
    }

    fn dark() -> Self {
        Theme {
            name: "dark".into(),
            bg: h("#0a0e27"),
            fg: h("#e0e0e0"),
            fg_dim: h("#666666"),
            accent: h("#00d4ff"),
            border: h("#333355"),
            border_focused: h("#00d4ff"),
            header_fg: h("#00d4ff"),
            header_accent: h("#00d4ff"),
            sidebar_selected_fg: h("#ffffff"),
            sidebar_unread: h("#ffcc00"),
            sidebar_status_online: h("#00ff88"),
            sidebar_channel_header: h("#00d4ff"),
            msg_own_sender: h("#00d4ff"),
            msg_peer_sender: h("#00ff88"),
            msg_timestamp: h("#666666"),
            msg_status_sent: h("#666666"),
            msg_status_delivered: h("#666666"),
            msg_status_read: h("#00ff88"),
            msg_status_failed: h("#ff4444"),
            msg_ephemeral: h("#ffcc00"),
            input_fg: h("#ffffff"),
            input_placeholder: h("#666666"),
            modal_border: h("#00d4ff"),
            modal_title: h("#00d4ff"),
            error: h("#ff4444"),
            warning: h("#ffcc00"),
            success: h("#00ff88"),
            channel_border: h("#cc44ff"),
            channel_read_count: h("#666666"),
        }
    }

    fn light() -> Self {
        Theme {
            name: "light".into(),
            bg: h("#f5f5f5"),
            fg: h("#1a1a1a"),
            fg_dim: h("#888888"),
            accent: h("#0066cc"),
            border: h("#cccccc"),
            border_focused: h("#0066cc"),
            header_fg: h("#0066cc"),
            header_accent: h("#0066cc"),
            sidebar_selected_fg: h("#000000"),
            sidebar_unread: h("#cc6600"),
            sidebar_status_online: h("#008844"),
            sidebar_channel_header: h("#0066cc"),
            msg_own_sender: h("#0066cc"),
            msg_peer_sender: h("#008844"),
            msg_timestamp: h("#888888"),
            msg_status_sent: h("#888888"),
            msg_status_delivered: h("#888888"),
            msg_status_read: h("#008844"),
            msg_status_failed: h("#cc0000"),
            msg_ephemeral: h("#cc6600"),
            input_fg: h("#1a1a1a"),
            input_placeholder: h("#888888"),
            modal_border: h("#0066cc"),
            modal_title: h("#0066cc"),
            error: h("#cc0000"),
            warning: h("#cc6600"),
            success: h("#008844"),
            channel_border: h("#8844aa"),
            channel_read_count: h("#888888"),
        }
    }

    fn cyberpunk() -> Self {
        Theme {
            name: "cyberpunk".into(),
            bg: h("#000000"),
            fg: h("#00ff41"),
            fg_dim: h("#005500"),
            accent: h("#00ff41"),
            border: h("#003300"),
            border_focused: h("#00ff41"),
            header_fg: h("#00ff41"),
            header_accent: h("#00ff41"),
            sidebar_selected_fg: h("#00ff41"),
            sidebar_unread: h("#ffff00"),
            sidebar_status_online: h("#00ff41"),
            sidebar_channel_header: h("#00ff41"),
            msg_own_sender: h("#00ff41"),
            msg_peer_sender: h("#00cc33"),
            msg_timestamp: h("#005500"),
            msg_status_sent: h("#005500"),
            msg_status_delivered: h("#005500"),
            msg_status_read: h("#00ff41"),
            msg_status_failed: h("#ff0000"),
            msg_ephemeral: h("#ffff00"),
            input_fg: h("#00ff41"),
            input_placeholder: h("#005500"),
            modal_border: h("#00ff41"),
            modal_title: h("#00ff41"),
            error: h("#ff0000"),
            warning: h("#ffff00"),
            success: h("#00ff41"),
            channel_border: h("#00ff41"),
            channel_read_count: h("#005500"),
        }
    }

    fn minimal() -> Self {
        Theme {
            name: "minimal".into(),
            bg: h("#000000"),
            fg: h("#cccccc"),
            fg_dim: h("#555555"),
            accent: h("#ffffff"),
            border: h("#333333"),
            border_focused: h("#ffffff"),
            header_fg: h("#ffffff"),
            header_accent: h("#ffffff"),
            sidebar_selected_fg: h("#ffffff"),
            sidebar_unread: h("#ffffff"),
            sidebar_status_online: h("#aaaaaa"),
            sidebar_channel_header: h("#ffffff"),
            msg_own_sender: h("#ffffff"),
            msg_peer_sender: h("#aaaaaa"),
            msg_timestamp: h("#555555"),
            msg_status_sent: h("#555555"),
            msg_status_delivered: h("#888888"),
            msg_status_read: h("#ffffff"),
            msg_status_failed: h("#888888"),
            msg_ephemeral: h("#aaaaaa"),
            input_fg: h("#cccccc"),
            input_placeholder: h("#555555"),
            modal_border: h("#ffffff"),
            modal_title: h("#ffffff"),
            error: h("#ffffff"),
            warning: h("#aaaaaa"),
            success: h("#ffffff"),
            channel_border: h("#ffffff"),
            channel_read_count: h("#555555"),
        }
    }

    fn rose_pine() -> Self {
        Theme {
            name: "rose-pine".into(),
            bg: h("#191724"),
            fg: h("#e0def4"),
            fg_dim: h("#6e6a86"),
            accent: h("#ebbcba"),
            border: h("#26233a"),
            border_focused: h("#ebbcba"),
            header_fg: h("#e0def4"),
            header_accent: h("#ebbcba"),
            sidebar_selected_fg: h("#e0def4"),
            sidebar_unread: h("#f6c177"),
            sidebar_status_online: h("#9ccfd8"),
            sidebar_channel_header: h("#c4a7e7"),
            msg_own_sender: h("#ebbcba"),
            msg_peer_sender: h("#9ccfd8"),
            msg_timestamp: h("#6e6a86"),
            msg_status_sent: h("#6e6a86"),
            msg_status_delivered: h("#908caa"),
            msg_status_read: h("#9ccfd8"),
            msg_status_failed: h("#eb6f92"),
            msg_ephemeral: h("#f6c177"),
            input_fg: h("#e0def4"),
            input_placeholder: h("#6e6a86"),
            modal_border: h("#ebbcba"),
            modal_title: h("#ebbcba"),
            error: h("#eb6f92"),
            warning: h("#f6c177"),
            success: h("#9ccfd8"),
            channel_border: h("#c4a7e7"),
            channel_read_count: h("#6e6a86"),
        }
    }

    fn rose_pine_moon() -> Self {
        Theme {
            name: "rose-pine-moon".into(),
            bg: h("#232136"),
            fg: h("#e0def4"),
            fg_dim: h("#6e6a86"),
            accent: h("#ea9a97"),
            border: h("#393552"),
            border_focused: h("#ea9a97"),
            header_fg: h("#e0def4"),
            header_accent: h("#ea9a97"),
            sidebar_selected_fg: h("#e0def4"),
            sidebar_unread: h("#f6c177"),
            sidebar_status_online: h("#9ccfd8"),
            sidebar_channel_header: h("#c4a7e7"),
            msg_own_sender: h("#ea9a97"),
            msg_peer_sender: h("#9ccfd8"),
            msg_timestamp: h("#6e6a86"),
            msg_status_sent: h("#6e6a86"),
            msg_status_delivered: h("#908caa"),
            msg_status_read: h("#9ccfd8"),
            msg_status_failed: h("#eb6f92"),
            msg_ephemeral: h("#f6c177"),
            input_fg: h("#e0def4"),
            input_placeholder: h("#6e6a86"),
            modal_border: h("#ea9a97"),
            modal_title: h("#ea9a97"),
            error: h("#eb6f92"),
            warning: h("#f6c177"),
            success: h("#9ccfd8"),
            channel_border: h("#c4a7e7"),
            channel_read_count: h("#6e6a86"),
        }
    }

    fn rose_pine_dawn() -> Self {
        Theme {
            name: "rose-pine-dawn".into(),
            bg: h("#faf4ed"),
            fg: h("#575279"),
            fg_dim: h("#9893a5"),
            accent: h("#d7827e"),
            border: h("#f2e9e1"),
            border_focused: h("#d7827e"),
            header_fg: h("#575279"),
            header_accent: h("#d7827e"),
            sidebar_selected_fg: h("#575279"),
            sidebar_unread: h("#ea9d34"),
            sidebar_status_online: h("#56949f"),
            sidebar_channel_header: h("#907aa9"),
            msg_own_sender: h("#d7827e"),
            msg_peer_sender: h("#56949f"),
            msg_timestamp: h("#9893a5"),
            msg_status_sent: h("#9893a5"),
            msg_status_delivered: h("#797593"),
            msg_status_read: h("#56949f"),
            msg_status_failed: h("#b4637a"),
            msg_ephemeral: h("#ea9d34"),
            input_fg: h("#575279"),
            input_placeholder: h("#9893a5"),
            modal_border: h("#d7827e"),
            modal_title: h("#d7827e"),
            error: h("#b4637a"),
            warning: h("#ea9d34"),
            success: h("#56949f"),
            channel_border: h("#907aa9"),
            channel_read_count: h("#9893a5"),
        }
    }

    /// Apply TOML color overrides on top of the current theme.
    /// Each field is optional — only specified overrides are applied.
    pub fn apply_overrides(&mut self, overrides: &ThemeOverrides) {
        if let Some(c) = overrides.bg.as_deref().and_then(parse_hex_color) { self.bg = c; }
        if let Some(c) = overrides.fg.as_deref().and_then(parse_hex_color) { self.fg = c; }
        if let Some(c) = overrides.fg_dim.as_deref().and_then(parse_hex_color) { self.fg_dim = c; }
        if let Some(c) = overrides.accent.as_deref().and_then(parse_hex_color) { self.accent = c; }
        if let Some(c) = overrides.border.as_deref().and_then(parse_hex_color) { self.border = c; }
        if let Some(c) = overrides.border_focused.as_deref().and_then(parse_hex_color) { self.border_focused = c; }
        if let Some(c) = overrides.error.as_deref().and_then(parse_hex_color) { self.error = c; }
        if let Some(c) = overrides.warning.as_deref().and_then(parse_hex_color) { self.warning = c; }
        if let Some(c) = overrides.success.as_deref().and_then(parse_hex_color) { self.success = c; }
    }
}

/// Subset of theme colors that can be overridden via TOML config.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ThemeOverrides {
    pub bg: Option<String>,
    pub fg: Option<String>,
    pub fg_dim: Option<String>,
    pub accent: Option<String>,
    pub border: Option<String>,
    pub border_focused: Option<String>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub success: Option<String>,
}

/// TOML config file structure for `~/.config/chattor/theme.toml`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct ThemeConfig {
    pub preset: Option<String>,
    #[serde(default)]
    pub colors: ThemeOverrides,
}

/// Load theme from CLI flag, config file, or default.
///
/// Priority: CLI flag > config file > "dark" default.
pub fn load_theme(cli_preset: Option<&str>, config_path: &std::path::Path) -> Theme {
    if let Some(name) = cli_preset {
        return Theme::preset(name);
    }

    if let Ok(content) = std::fs::read_to_string(config_path) {
        if let Ok(config) = toml::from_str::<ThemeConfig>(&content) {
            let mut theme = Theme::preset(config.preset.as_deref().unwrap_or("dark"));
            theme.apply_overrides(&config.colors);
            return theme;
        }
    }

    Theme::preset("dark")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_parse_hex_color_valid() {
        assert_eq!(parse_hex_color("#00d4ff"), Some(Color::Rgb(0, 212, 255)));
        assert_eq!(parse_hex_color("#ffffff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_hex_color("#000000"), Some(Color::Rgb(0, 0, 0)));
    }

    #[test]
    fn test_parse_hex_color_invalid() {
        assert_eq!(parse_hex_color("not-a-color"), None);
        assert_eq!(parse_hex_color("#gggggg"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn test_dark_preset_has_expected_colors() {
        let theme = Theme::preset("dark");
        assert_eq!(theme.name, "dark");
        assert_eq!(theme.bg, Color::Rgb(10, 14, 39));
        assert_eq!(theme.accent, Color::Rgb(0, 212, 255));
    }

    #[test]
    fn test_all_presets_exist() {
        for name in Theme::preset_names() {
            let theme = Theme::preset(name);
            assert_eq!(&theme.name, name);
        }
    }

    #[test]
    fn test_unknown_preset_falls_back_to_dark() {
        let theme = Theme::preset("nonexistent");
        assert_eq!(theme.name, "dark");
    }

    #[test]
    fn test_apply_overrides() {
        let mut theme = Theme::preset("dark");
        let overrides = ThemeOverrides {
            accent: Some("#ff0000".into()),
            ..Default::default()
        };
        theme.apply_overrides(&overrides);
        assert_eq!(theme.accent, Color::Rgb(255, 0, 0));
        // Other fields unchanged
        assert_eq!(theme.bg, Color::Rgb(10, 14, 39));
    }

    #[test]
    fn test_load_theme_cli_takes_priority() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "preset = \"light\"").unwrap();
        let theme = load_theme(Some("cyberpunk"), tmp.path());
        assert_eq!(theme.name, "cyberpunk");
    }

    #[test]
    fn test_load_theme_from_config_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "preset = \"minimal\"\n\n[colors]\naccent = \"#abcdef\"").unwrap();
        let theme = load_theme(None, tmp.path());
        assert_eq!(theme.name, "minimal");
        assert_eq!(theme.accent, Color::Rgb(171, 205, 239));
    }

    #[test]
    fn test_load_theme_default_dark() {
        let theme = load_theme(None, std::path::Path::new("/nonexistent/path"));
        assert_eq!(theme.name, "dark");
    }

    #[test]
    fn test_theme_config_deserialization() {
        let toml_str = r#"
preset = "rose-pine"

[colors]
accent = "#00ffaa"
bg = "#111111"
"#;
        let config: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.preset.as_deref(), Some("rose-pine"));
        assert_eq!(config.colors.accent.as_deref(), Some("#00ffaa"));
        assert_eq!(config.colors.bg.as_deref(), Some("#111111"));
        assert!(config.colors.fg.is_none());
    }
}
```

**Step 2: Register the module in `src/ui/mod.rs`**

Add after the existing module declarations:
```rust
pub mod theme;
```

Add to the `pub use` section:
```rust
pub use theme::Theme;
```

**Step 3: Run tests**

Run: `cargo test ui::theme --lib`
Expected: All 9 tests pass

**Step 4: Commit**

```bash
git add src/ui/theme.rs src/ui/mod.rs
git commit -m "feat: add Theme struct with 7 preset themes, hex parsing, and TOML config"
```

---

## Task 2: Add --theme CLI flag

**Files:**
- Modify: `src/cli.rs`

**Step 1: Add the `--theme` argument to the Cli struct**

Add this field after `config_dir`:
```rust
    /// Theme preset (dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn)
    #[arg(short, long)]
    pub theme: Option<String>,
```

**Step 2: Add test**

Add to the existing tests module in `src/cli.rs`:
```rust
    #[test]
    fn test_cli_theme_flag() {
        let cli = Cli::parse_from(["chattor", "--theme", "cyberpunk"]);
        assert_eq!(cli.theme.as_deref(), Some("cyberpunk"));
    }

    #[test]
    fn test_cli_no_theme() {
        let cli = Cli::parse_from(["chattor"]);
        assert!(cli.theme.is_none());
    }
```

**Step 3: Run tests**

Run: `cargo test cli --lib`
Expected: All 3 CLI tests pass

**Step 4: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add --theme CLI flag for preset selection"
```

---

## Task 3: Wire Theme through RenderContext and main.rs

This task threads the Theme through the application without changing any colors yet.

**Files:**
- Modify: `src/ui/app_ui.rs` — add `theme: Theme` field to `RenderContext`
- Modify: `src/main.rs` — load theme at startup, pass into RenderContext, pass to bootstrap screens

**Step 1: Add `theme` field to `RenderContext` in `src/ui/app_ui.rs`**

Add to the RenderContext struct (after the other imports at the top, add `use super::Theme;`):
```rust
    pub theme: Theme,
```

**Step 2: Update `src/main.rs` — theme loading and RenderContext population**

At the top of `main()`, after `let _cli = Cli::parse();`, change to:
```rust
    let cli = Cli::parse();
```

(Remove the underscore prefix so we can use it.)

After the App initialization (`let app = Arc::new(...)`), add theme loading:
```rust
    // Load theme
    let theme = {
        let app_lock = app.lock().await;
        let config_path = app_lock.settings.config_dir.join("theme.toml");
        drop(app_lock);
        ui::theme::load_theme(cli.theme.as_deref(), &config_path)
    };
```

In the RenderContext construction (around line 241), add the theme field:
```rust
        let ctx = RenderContext {
            friends,
            messages,
            own_onion,
            friend_code,
            tor_connected,
            pending_request_count,
            conversation_ephemeral_ttl,
            channel_subscriptions,
            channel_posts,
            channel_post_read_counts,
            theme: theme.clone(),
        };
```

**Step 3: Compile check**

Run: `cargo build`
Expected: Compiles cleanly

**Step 4: Commit**

```bash
git add src/ui/app_ui.rs src/main.rs
git commit -m "feat: wire Theme through RenderContext and load at startup"
```

---

## Task 4: Apply theme to header, footer, and sidebar

Replace all hardcoded colors in `app_ui.rs` and `sidebar.rs` with theme references.

**Files:**
- Modify: `src/ui/app_ui.rs`
- Modify: `src/ui/sidebar.rs`

**Step 1: Update `render_app` in `src/ui/app_ui.rs`**

Add `BorderType` to ratatui imports:
```rust
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
```

Replace the header rendering section with:
```rust
    let header = Paragraph::new(format!("  chattor v0.1.0{}  [Tor: {}]", addr_display, tor_status))
        .style(Style::default().fg(ctx.theme.header_accent))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(ctx.theme.border)));
```

Replace the sidebar width from `Constraint::Length(20)` to `Constraint::Length(24)`.

Pass `&ctx.theme` to all sub-render function calls. Update each call site:

For sidebar:
```rust
        crate::ui::sidebar::render_sidebar_with_channels(
            f, main_chunks[0], &ctx.friends, selected_idx, !input_focused,
            ctx.pending_request_count, &ctx.channel_subscriptions, &ctx.theme,
        );
```

For conversation:
```rust
        crate::ui::conversation::render_conversation(
            f, right_chunks[0], selected_friend, &ctx.messages,
            ctx.own_onion.as_deref(), scroll_offset, ctx.conversation_ephemeral_ttl, &ctx.theme,
        );
```

For input:
```rust
        crate::ui::conversation::render_input(
            f, right_chunks[1], input, cursor, input_focused, &ctx.theme,
        );
```

For channel feed:
```rust
        crate::ui::channel_feed::render_channel_feed(
            f, chunks[1], publisher_onion, channel_type, *is_own,
            input, *cursor, *scroll_offset,
            &ctx.channel_posts, &ctx.channel_post_read_counts, &ctx.theme,
        );
```

For setup wizard (which we'll replace in Task 9, but for now just pass theme):
```rust
        crate::ui::conversation::render_setup_wizard(f, chunks[1], onion_ref, code_ref, &ctx.theme);
```

Replace the footer rendering with themed keybinding hints:
```rust
    // Footer with themed keybinding hints
    let footer_spans = format_footer_spans(app_state, &ctx.theme);
    let footer = Paragraph::new(ratatui::text::Line::from(footer_spans));
    f.render_widget(footer, chunks[2]);
```

Add a helper function to `app_ui.rs`:
```rust
fn format_footer_spans<'a>(state: &AppState, theme: &'a Theme) -> Vec<ratatui::text::Span<'a>> {
    let pairs: Vec<(&str, &str)> = match state {
        AppState::Normal { input_focused: true, .. } => vec![("Enter", "Send"), ("Esc", "Nav")],
        AppState::Normal { .. } => vec![("Tab/↑↓", "Select"), ("Enter", "Open"), ("a", "Add"), ("e", "Ephemeral"), ("i", "Identity"), ("f", "Requests"), ("q", "Quit")],
        AppState::AddingFriend { .. } => vec![("Enter", "Send"), ("Esc", "Cancel")],
        AppState::ViewingFriendRequests { .. } => vec![("↑↓", "Navigate"), ("Enter", "View"), ("Esc", "Back")],
        AppState::ViewingFriendRequest { .. } => vec![("A", "Accept"), ("R", "Reject"), ("Esc", "Back")],
        AppState::ViewingMyIdentity { .. } => vec![("i/Esc", "Close")],
        AppState::SettingEphemeral { .. } => vec![("↑↓", "Select"), ("Enter", "Confirm"), ("Esc", "Cancel")],
        AppState::ViewingChannel { is_own: true, .. } => vec![("Enter", "Post"), ("Esc", "Back")],
        AppState::ViewingChannel { .. } => vec![("Esc", "Back")],
        AppState::SubscribingToChannel { .. } => vec![("Enter", "Subscribe"), ("Esc", "Cancel")],
    };

    let mut spans = vec![ratatui::text::Span::raw("  ")];
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(ratatui::text::Span::styled("  ", Style::default().fg(theme.fg_dim)));
        }
        spans.push(ratatui::text::Span::styled(
            format!("[{}]", key),
            Style::default().fg(theme.accent),
        ));
        spans.push(ratatui::text::Span::styled(
            format!(" {}", desc),
            Style::default().fg(theme.fg_dim),
        ));
    }
    spans
}
```

For modal overlays, pass `&ctx.theme` to each modal render function:
```rust
        AppState::AddingFriend { input, error, .. } => {
            crate::ui::modals::render_add_friend_modal(f, input, error.as_deref(), &ctx.theme);
        }
        AppState::ViewingFriendRequests { requests, selected_idx } => {
            crate::ui::modals::render_friend_request_list(f, requests, *selected_idx, &ctx.theme);
        }
        AppState::ViewingFriendRequest { from_onion, friend_code, .. } => {
            crate::ui::modals::render_friend_request_modal(f, from_onion, friend_code, &ctx.theme);
        }
        AppState::ViewingMyIdentity { friend_code, onion_address } => {
            crate::ui::modals::render_identity_modal(f, friend_code, onion_address, &ctx.theme);
        }
        AppState::SettingEphemeral { selected_idx, .. } => {
            crate::ui::modals::render_ephemeral_modal(f, *selected_idx, &ctx.theme);
        }
        AppState::SubscribingToChannel { input, error, .. } => {
            crate::ui::modals::render_subscribe_channel_modal(f, input, error.as_deref(), &ctx.theme);
        }
```

**Step 2: Update `render_sidebar_with_channels` and friends in `src/ui/sidebar.rs`**

Add `Theme` import and `BorderType`:
```rust
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
    Frame,
};
use crate::ui::theme::Theme;
```

Add `theme: &Theme` parameter to `render_sidebar`:
```rust
pub fn render_sidebar(
    f: &mut Frame,
    area: Rect,
    friends: &[FriendEntry],
    selected_idx: Option<usize>,
    focused: bool,
    pending_request_count: i64,
    theme: &Theme,
) {
    render_sidebar_with_channels(f, area, friends, selected_idx, focused, pending_request_count, &[], theme);
}
```

Add `theme: &Theme` parameter to `render_sidebar_with_channels`:
```rust
pub fn render_sidebar_with_channels(
    f: &mut Frame,
    area: Rect,
    friends: &[FriendEntry],
    selected_idx: Option<usize>,
    focused: bool,
    pending_request_count: i64,
    channel_subscriptions: &[ChannelSubscription],
    theme: &Theme,
) {
    // ... body unchanged, just pass theme to sub-functions:
    render_friends_list(f, sidebar_chunks[0], friends, selected_idx, focused, pending_request_count, theme);
    render_channels_section(f, sidebar_chunks[1], channel_subscriptions, theme);
}
```

In `render_friends_list`, add `theme: &Theme` parameter and replace colors:
- `Color::Yellow` (pending) → `theme.warning`
- `Color::Cyan` (focused) → `theme.border_focused`
- `Color::DarkGray` (unfocused border) → `theme.border`
- `Color::White` + BOLD (selected) → `theme.sidebar_selected_fg` + BOLD
- `Color::Gray` (unselected) → `theme.fg`
- `Color::DarkGray` (status icon) → `theme.fg_dim`
- `Color::Yellow` + BOLD (unread) → `theme.sidebar_unread` + BOLD
- Increase `max_name_len` from `10` to `14` (wider sidebar = more room)
- Add `.border_type(BorderType::Rounded)` to the block

In `render_channels_section`, add `theme: &Theme` parameter and replace:
- `Color::Cyan` + BOLD (headers) → `theme.sidebar_channel_header` + BOLD
- `Color::Gray` (items) → `theme.fg`
- `Color::DarkGray` (border) → `theme.border`
- Add `.border_type(BorderType::Rounded)` to the block

**Step 3: Update `src/ui/mod.rs` exports**

The `render_sidebar` and `render_sidebar_with_channels` function signatures now include `theme`, so update their `pub use` re-exports. Since these are called from `app_ui.rs` via `crate::ui::sidebar::`, the re-exports in mod.rs don't need parameter changes — they just re-export the names. No change needed in mod.rs for this.

**Step 4: Compile check**

Run: `cargo build`
Expected: Will fail because conversation.rs, channel_feed.rs, modals.rs don't accept theme yet. That's OK — proceed to the next tasks. Alternatively, update all function signatures first (adding `theme: &Theme` and `_theme` if not yet used) so it compiles.

To make it compile, temporarily add `theme: &Theme` to all remaining render functions, prefixed with `_` if unused:

- `render_conversation` — add `_theme: &Theme`
- `render_input` — add `_theme: &Theme`
- `render_setup_wizard` — add `_theme: &Theme`
- `render_channel_feed` — add `_theme: &Theme`
- All modal functions — add `_theme: &Theme`

**Step 5: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 6: Commit**

```bash
git add src/ui/app_ui.rs src/ui/sidebar.rs src/ui/conversation.rs src/ui/channel_feed.rs src/ui/modals.rs
git commit -m "feat: apply theme to header, footer, and sidebar; widen sidebar to 24"
```

---

## Task 5: Apply theme to conversation and channel feed

**Files:**
- Modify: `src/ui/conversation.rs`
- Modify: `src/ui/channel_feed.rs`

**Step 1: Theme `render_conversation` in `src/ui/conversation.rs`**

Add imports:
```rust
use ratatui::widgets::BorderType;
use crate::ui::theme::Theme;
```

Update `render_conversation` signature to take `theme: &Theme` (replace `_theme` with `theme`).

Replace colors:
- Conversation border `Color::DarkGray` → `theme.border`
- "Select a friend" / "No messages yet" text `Color::DarkGray` → `theme.fg_dim`
- Add `.border_type(BorderType::Rounded)` to conversation block
- Pass `theme` to `render_messages`

In `render_messages`, add `theme: &Theme` parameter and replace:
- Own sender `Color::Cyan` + BOLD → `theme.msg_own_sender` + BOLD
- Peer sender `Color::Green` + BOLD → `theme.msg_peer_sender` + BOLD
- Timestamp `Color::DarkGray` → `theme.msg_timestamp`
- Status "sent"/"queued" `Color::DarkGray` → `theme.msg_status_sent`
- Status "failed" `Color::Red` → `theme.msg_status_failed`
- Status "delivered" `Color::DarkGray` → `theme.msg_status_delivered`
- Status "read" `Color::Green` → `theme.msg_status_read`

Update `render_input` to use `theme: &Theme` (replace `_theme`):
- Focused border `Color::Cyan` → `theme.border_focused`
- Unfocused border `Color::DarkGray` → `theme.border`
- Focused text `Color::White` → `theme.input_fg`
- Unfocused text `Color::DarkGray` → `theme.input_placeholder`
- Add `.border_type(BorderType::Rounded)` to the block
- When not focused and empty, show `"Type a message..."` in `theme.input_placeholder` (replaces old text)

Update `render_setup_wizard` to use `theme: &Theme` (replace `_theme`):
- `Color::Cyan` + BOLD → `theme.accent` + BOLD
- `Color::White` → `theme.fg`
- `Color::Green` + BOLD → `theme.success` + BOLD
- `Color::Yellow` → `theme.warning`
- `Color::DarkGray` → `theme.border`
- Add `.border_type(BorderType::Rounded)` to blocks

**Step 2: Theme `render_channel_feed` in `src/ui/channel_feed.rs`**

Add imports:
```rust
use ratatui::widgets::BorderType;
use crate::ui::theme::Theme;
```

Update `render_channel_feed` signature to take `theme: &Theme` (replace `_theme`). Pass to sub-functions.

In `render_posts`:
- Border `Color::Magenta` → `theme.channel_border`
- Timestamp `Color::DarkGray` → `theme.msg_timestamp`
- Read count `Color::DarkGray` → `theme.channel_read_count`
- Content `Color::White` → `theme.fg`
- "No posts yet" text `Color::DarkGray` → `theme.fg_dim`
- Add `.border_type(BorderType::Rounded)` to block

In `render_channel_input`:
- Border `Color::Magenta` → `theme.channel_border`
- Text `Color::White` → `theme.input_fg`
- Add `.border_type(BorderType::Rounded)` to block

**Step 3: Compile and test**

Run: `cargo build && cargo test`
Expected: Compiles, all tests pass

**Step 4: Commit**

```bash
git add src/ui/conversation.rs src/ui/channel_feed.rs
git commit -m "feat: apply theme to conversation, input, and channel feed"
```

---

## Task 6: Apply theme to modals

**Files:**
- Modify: `src/ui/modals.rs`

**Step 1: Add imports and update all modal function signatures**

Add to imports:
```rust
use ratatui::widgets::BorderType;
use crate::ui::theme::Theme;
```

Update each function to take `theme: &Theme` instead of `_theme: &Theme`.

**Step 2: Replace colors in each modal**

`render_add_friend_modal`:
- Border `Color::Cyan` → `theme.modal_border`
- Input text `Color::White` → `theme.input_fg`
- Error `Color::Red` → `theme.error`
- Help text `Color::Gray` → `theme.fg_dim`
- Controls `Color::Gray` → `theme.fg_dim`
- Add `.border_type(BorderType::Rounded)` to all blocks

`render_friend_request_modal`:
- Border `Color::Yellow` → `theme.warning`
- Help text `Color::Gray` → `theme.fg_dim`
- Controls `Color::Gray` → `theme.fg_dim`
- Add `.border_type(BorderType::Rounded)`

`render_friend_request_list`:
- Border `Color::Yellow` → `theme.warning`
- Empty text `Color::Gray` → `theme.fg_dim`
- Selected `Color::White` + BOLD → `theme.sidebar_selected_fg` + BOLD
- Unselected `Color::Gray` → `theme.fg`
- Time `Color::DarkGray` → `theme.fg_dim`
- Controls `Color::Gray` → `theme.fg_dim`
- Add `.border_type(BorderType::Rounded)`

`render_identity_modal`:
- Border `Color::Cyan` → `theme.modal_border`
- "Share this address" label `Color::White` → `theme.fg`
- Onion address `Color::Green` + BOLD → `theme.success` + BOLD
- "Friend Code:" label `Color::DarkGray` → `theme.fg_dim`
- Friend code `Color::Yellow` → `theme.warning`
- Help text `Color::DarkGray` → `theme.fg_dim`
- Add `.border_type(BorderType::Rounded)` to all blocks

`render_ephemeral_modal`:
- Border `Color::Cyan` → `theme.modal_border`
- Selected `Color::White` + BOLD → `theme.sidebar_selected_fg` + BOLD
- Unselected `Color::Gray` → `theme.fg`
- Controls `Color::Gray` → `theme.fg_dim`
- Add `.border_type(BorderType::Rounded)`

`render_subscribe_channel_modal`:
- Border `Color::Magenta` → `theme.channel_border`
- Input `Color::White` → `theme.input_fg`
- Error `Color::Red` → `theme.error`
- Help text `Color::Gray` → `theme.fg_dim`
- Controls `Color::Gray` → `theme.fg_dim`
- Add `.border_type(BorderType::Rounded)` to all blocks

**Step 3: Compile and test**

Run: `cargo build && cargo test`
Expected: Compiles, all tests pass

**Step 4: Commit**

```bash
git add src/ui/modals.rs
git commit -m "feat: apply theme to all modal dialogs"
```

---

## Task 7: Apply theme to bootstrap screen

**Files:**
- Modify: `src/ui/bootstrap.rs`
- Modify: `src/main.rs` — pass theme to bootstrap render calls

**Step 1: Update bootstrap render functions**

Add to imports:
```rust
use crate::ui::theme::Theme;
```

Update `render_connecting` signature:
```rust
pub fn render_connecting(f: &mut Frame, frame: usize, tick: u64, _progress: u8, theme: &Theme) {
```

Replace colors:
- Title `Color::Cyan` → `theme.accent`
- Art lines `Color::Cyan` → `theme.accent`
- Status message `Color::DarkGray` → `theme.fg_dim`

Update `render_failure` signature:
```rust
pub fn render_failure(f: &mut Frame, error: &str, theme: &Theme) {
```

Replace colors:
- Title `Color::DarkGray` → `theme.fg_dim`
- Art `Color::DarkGray` → `theme.fg_dim`
- "connection failed" `Color::Red` + BOLD → `theme.error` + BOLD
- Error detail `Color::DarkGray` → `theme.fg_dim`
- Tips `Color::Gray` → `theme.fg`
- Docs link `Color::DarkGray` → `theme.fg_dim`
- Action key `Color::Cyan` + BOLD → `theme.accent` + BOLD
- Action label `Color::Gray` → `theme.fg`

**Step 2: Update `src/main.rs` bootstrap render calls**

In the bootstrap loop, pass `&theme` to render calls:
```rust
                terminal.draw(|fr| {
                    ui::render_connecting(fr, f, t, p, &theme);
                })?;
```
```rust
                terminal.draw(|fr| {
                    ui::render_failure(fr, &err, &theme);
                })?;
```

**Step 3: Update `src/ui/mod.rs` re-exports**

The re-exports of `render_connecting` and `render_failure` should still work since they reference the function names. But check that the call sites in main.rs use the updated signatures.

**Step 4: Compile and test**

Run: `cargo build && cargo test`
Expected: Compiles, all tests pass

**Step 5: Commit**

```bash
git add src/ui/bootstrap.rs src/main.rs
git commit -m "feat: apply theme to bootstrap connecting and failure screens"
```

---

## Task 8: Visual polish — padding, placeholder text, connection status

This task adds the remaining visual polish items from the design doc.

**Files:**
- Modify: `src/ui/app_ui.rs` — connection status indicator, padding
- Modify: `src/ui/conversation.rs` — inner padding, placeholder text

**Step 1: Connection status indicator in header**

In `render_app`, update the header to show a styled connection indicator:
```rust
    let (tor_icon, tor_label, tor_color) = if ctx.tor_connected {
        ("◉", "Connected", ctx.theme.success)
    } else {
        ("◌", "Connecting...", ctx.theme.warning)
    };
```

Build the header with styled spans:
```rust
    use ratatui::text::{Line, Span};
    let header_line = Line::from(vec![
        Span::styled("  chattor", Style::default().fg(ctx.theme.header_accent).add_modifier(ratatui::style::Modifier::BOLD)),
        Span::styled(addr_display, Style::default().fg(ctx.theme.fg_dim)),
        Span::raw("  "),
        Span::styled(format!("{} {}", tor_icon, tor_label), Style::default().fg(tor_color)),
    ]);
    let header = Paragraph::new(header_line)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(ctx.theme.border)));
```

**Step 2: Inner padding in conversation area**

In `render_conversation`, after computing `inner` from the block, add 1-char horizontal padding:
```rust
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Add 1-char horizontal padding
    let padded = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };
```

Use `padded` instead of `inner` for all content rendering within the conversation.

**Step 3: Placeholder text in input**

In `render_input`, when not focused and empty, show themed placeholder:
```rust
    } else {
        if input.is_empty() {
            format!("{}Type a message...", prompt)
        } else {
            format!("{}{}", prompt, input)
        }
    };
```

This should already be done in the previous task where we replaced the old text. Verify.

**Step 4: Compile and test**

Run: `cargo build && cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/ui/app_ui.rs src/ui/conversation.rs
git commit -m "feat: add connection status indicator, inner padding, and placeholder text"
```

---

## Task 9: Remove setup wizard, add empty state hint

The design says: remove the full-screen setup wizard. Instead, show the same sidebar + conversation layout with an empty state message.

**Files:**
- Modify: `src/ui/app_ui.rs` — remove setup wizard branch, show empty state
- Modify: `src/ui/conversation.rs` — update "no conversation selected" empty state

**Step 1: Update `render_app` in `src/ui/app_ui.rs`**

Remove the entire `if ctx.friends.is_empty() && ctx.channel_subscriptions.is_empty()` branch that calls `render_setup_wizard`. Instead, always show the sidebar + conversation layout. The conversation area will show the empty state hint when no friend is selected.

Replace:
```rust
    } else if ctx.friends.is_empty() && ctx.channel_subscriptions.is_empty() {
        // Setup wizard
        let (onion_ref, code_ref) = (ctx.own_onion.as_deref(), ctx.friend_code.as_deref());
        crate::ui::conversation::render_setup_wizard(f, chunks[1], onion_ref, code_ref, &ctx.theme);
    } else {
```

With just:
```rust
    } else {
```

So both the "has friends" and "no friends" cases go through the same sidebar + conversation path.

**Step 2: Update empty state in `render_conversation`**

When `friend` is `None` (no conversation selected), show a helpful hint instead of just "Select a friend":

```rust
        None => {
            let hint = if messages.is_empty() {
                vec![
                    Line::from(Span::styled("Welcome to chattor", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
                    Line::from(""),
                    Line::from(Span::styled("Press [a] to add a friend", Style::default().fg(theme.fg))),
                    Line::from(Span::styled("Press [i] to view your identity", Style::default().fg(theme.fg))),
                ]
            } else {
                vec![Line::from(Span::styled("Select a friend to start chatting", Style::default().fg(theme.fg_dim)))]
            };

            let text = Paragraph::new(hint)
                .alignment(Alignment::Center);

            let v_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(40),
                    Constraint::Length(4),
                    Constraint::Percentage(40),
                ])
                .split(padded);
            f.render_widget(text, v_layout[1]);
        }
```

Note: The `messages.is_empty()` check here is just a proxy for "no friends at all" since we're not in a conversation. We can simplify this to always show the hint since `friend` is `None`.

**Step 3: Remove `render_setup_wizard` function from `src/ui/conversation.rs`**

Delete the entire `render_setup_wizard` function (lines ~135-190 approximately). Also remove it from `src/ui/mod.rs` re-exports.

**Step 4: Update `src/ui/mod.rs`**

Remove `render_setup_wizard` from the `pub use conversation::` line.

**Step 5: Remove setup wizard click handling in `src/main.rs`**

In the `Event::Mouse` handler (around line 578-604), remove the block that checks `if friends.is_empty()` and tries to handle clicks on the setup wizard identity boxes. This code is no longer needed since the setup wizard is gone.

**Step 6: Compile and test**

Run: `cargo build && cargo test`
Expected: Compiles, all tests pass

**Step 7: Commit**

```bash
git add src/ui/app_ui.rs src/ui/conversation.rs src/ui/mod.rs src/main.rs
git commit -m "feat: remove setup wizard, add empty state hint in conversation area"
```

---

## Task 10: Polish identity modal with copy feedback

Redesign the identity modal with cleaner layout and visual copy feedback.

**Files:**
- Modify: `src/ui/modals.rs` — redesigned identity modal
- Modify: `src/ui/state.rs` — add `copied_field` to ViewingMyIdentity state
- Modify: `src/ui/app_ui.rs` — pass copied_field to modal

**Step 1: Add `copied_field` to `ViewingMyIdentity` state in `src/ui/state.rs`**

Update the enum variant:
```rust
    ViewingMyIdentity {
        friend_code: String,
        onion_address: String,
        copied_field: Option<String>, // "onion" or "code" — shown briefly after copy
    },
```

In the `handle_key` implementation for `ViewingMyIdentity`, add copy-on-keypress:
```rust
            AppState::ViewingMyIdentity { ref onion_address, ref friend_code, ref mut copied_field } => {
                match key.code {
                    KeyCode::Char('i') | KeyCode::Esc => {
                        *self = AppState::default();
                    }
                    KeyCode::Char('o') | KeyCode::Char('1') => {
                        if crate::ui::copy_to_clipboard(onion_address) {
                            *copied_field = Some("onion".into());
                        }
                    }
                    KeyCode::Char('c') | KeyCode::Char('2') => {
                        if crate::ui::copy_to_clipboard(friend_code) {
                            *copied_field = Some("code".into());
                        }
                    }
                    _ => {}
                }
                Ok(None)
            }
```

Update the `ViewingMyIdentity` construction in `src/main.rs` (in the `ViewMyIdentity` action handler) to include `copied_field: None`.

**Step 2: Update `render_identity_modal` in `src/ui/modals.rs`**

Update signature to accept the copied field:
```rust
pub fn render_identity_modal(f: &mut Frame, friend_code: &str, onion_address: &str, copied_field: Option<&str>, theme: &Theme) {
```

Redesign the layout:
```rust
pub fn render_identity_modal(f: &mut Frame, friend_code: &str, onion_address: &str, copied_field: Option<&str>, theme: &Theme) {
    use ratatui::style::Modifier;

    let area = centered_rect(60, 50, f.size());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" My Identity ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.modal_border));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),  // Label
            Constraint::Length(3),  // Onion address box
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Label
            Constraint::Length(3),  // Friend code box
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Help text
        ])
        .split(inner);

    // Onion address section
    let onion_label = if copied_field == Some("onion") {
        "Onion Address  [Copied!]"
    } else {
        "Onion Address  [o] copy"
    };
    let label_color = if copied_field == Some("onion") { theme.success } else { theme.fg };
    let label1 = Paragraph::new(onion_label)
        .style(Style::default().fg(label_color));
    f.render_widget(label1, chunks[0]);

    let onion_widget = Paragraph::new(onion_address)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.success).add_modifier(Modifier::BOLD))
        .wrap(Wrap { trim: false });
    f.render_widget(onion_widget, chunks[1]);

    // Friend code section
    let code_label = if copied_field == Some("code") {
        "Friend Code  [Copied!]"
    } else {
        "Friend Code  [c] copy"
    };
    let code_label_color = if copied_field == Some("code") { theme.success } else { theme.fg_dim };
    let label2 = Paragraph::new(code_label)
        .style(Style::default().fg(code_label_color));
    f.render_widget(label2, chunks[3]);

    let code_widget = Paragraph::new(friend_code)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .style(Style::default().fg(theme.warning))
        .wrap(Wrap { trim: false });
    f.render_widget(code_widget, chunks[4]);

    // Help text
    let help = Paragraph::new("[Esc/i] Close")
        .style(Style::default().fg(theme.fg_dim));
    f.render_widget(help, chunks[6]);
}
```

**Step 3: Update the modal overlay call in `app_ui.rs`**

```rust
        AppState::ViewingMyIdentity { friend_code, onion_address, copied_field } => {
            crate::ui::modals::render_identity_modal(f, friend_code, onion_address, copied_field.as_deref(), &ctx.theme);
        }
```

**Step 4: Compile and test**

Run: `cargo build && cargo test`
Expected: Compiles, all tests pass

**Step 5: Commit**

```bash
git add src/ui/modals.rs src/ui/state.rs src/ui/app_ui.rs src/main.rs
git commit -m "feat: polish identity modal with copy feedback and themed layout"
```

---

## Task 11: Fix clipboard functionality

Debug and ensure the `copy_to_clipboard` function works on Linux (X11/Wayland).

**Files:**
- Modify: `src/ui/mod.rs` — improve clipboard error handling

**Step 1: Investigate and fix clipboard**

The current implementation uses `arboard::Clipboard::new()` which should work. Common issues:
- On Wayland, `arboard` requires `wl-copy` to be available, or needs the `wayland-data-control` feature
- On X11, it requires an X11 connection

Update `copy_to_clipboard` to try both approaches and log errors for debugging:

```rust
/// Copy text to system clipboard. Returns true on success.
pub fn copy_to_clipboard(text: &str) -> bool {
    // Try arboard first
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            match clipboard.set_text(text) {
                Ok(()) => return true,
                Err(_e) => {
                    // arboard failed, try command-line fallback
                }
            }
        }
        Err(_e) => {
            // arboard init failed, try command-line fallback
        }
    }

    // Fallback: try command-line clipboard tools
    clipboard_fallback(text)
}

/// Fallback clipboard using command-line tools (wl-copy, xclip, xsel, pbcopy).
fn clipboard_fallback(text: &str) -> bool {
    use std::process::{Command, Stdio};
    use std::io::Write;

    // Try in order: wl-copy (Wayland), xclip (X11), xsel (X11), pbcopy (macOS)
    let tools = [
        ("wl-copy", &[] as &[&str]),
        ("xclip", &["-selection", "clipboard"] as &[&str]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
    ];

    for (cmd, args) in &tools {
        if let Ok(mut child) = Command::new(cmd)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                if stdin.write_all(text.as_bytes()).is_ok() {
                    drop(stdin);
                    if let Ok(status) = child.wait() {
                        if status.success() {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}
```

**Step 2: Compile and test**

Run: `cargo build && cargo test`
Expected: Compiles, all tests pass

**Step 3: Commit**

```bash
git add src/ui/mod.rs
git commit -m "fix: add clipboard fallback using wl-copy/xclip/xsel/pbcopy"
```

---

## Task 12: Update CLAUDE.md documentation

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update documentation**

Add to the Phase Implementation Status section:
```markdown
### Phase 4: Polish & Theming ✅
- Full theming system with Theme struct and 7 preset themes
- TOML config overrides via ~/.config/chattor/theme.toml
- --theme CLI flag for quick preset switching
- All UI components themed (sidebar, conversation, modals, bootstrap, channels)
- Rounded borders throughout
- Wider sidebar (24 chars)
- Connection status indicator in header (◉/◌)
- Themed keybinding hints in footer
- Polished identity modal with copy feedback ([o] copy onion, [c] copy code)
- Setup wizard removed, replaced with empty state hint
- Clipboard fix with fallback to wl-copy/xclip/xsel/pbcopy
```

Update the test count and any other relevant sections.

Add theme config details to the Platform-Specific Paths or Key Files section:
```markdown
- `src/ui/theme.rs` - Theme struct, 7 preset definitions, hex color parsing, TOML config loading
```

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with Phase 4 completion details"
```

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Theme struct + presets + hex parsing + TOML config | `src/ui/theme.rs` (new), `src/ui/mod.rs` |
| 2 | CLI --theme flag | `src/cli.rs` |
| 3 | Wire theme through RenderContext + main.rs | `src/ui/app_ui.rs`, `src/main.rs` |
| 4 | Theme header, footer, sidebar | `src/ui/app_ui.rs`, `src/ui/sidebar.rs`, + stub params in other files |
| 5 | Theme conversation + channel feed | `src/ui/conversation.rs`, `src/ui/channel_feed.rs` |
| 6 | Theme modals | `src/ui/modals.rs` |
| 7 | Theme bootstrap screen | `src/ui/bootstrap.rs`, `src/main.rs` |
| 8 | Visual polish (status indicator, padding, placeholder) | `src/ui/app_ui.rs`, `src/ui/conversation.rs` |
| 9 | Remove setup wizard, add empty state | `src/ui/app_ui.rs`, `src/ui/conversation.rs`, `src/ui/mod.rs`, `src/main.rs` |
| 10 | Polish identity modal with copy feedback | `src/ui/modals.rs`, `src/ui/state.rs`, `src/ui/app_ui.rs`, `src/main.rs` |
| 11 | Fix clipboard | `src/ui/mod.rs` |
| 12 | Update CLAUDE.md | `CLAUDE.md` |
