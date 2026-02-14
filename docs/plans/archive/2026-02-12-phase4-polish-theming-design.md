# Phase 4: Polish & Theming Design

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform chattor's functional but plain TUI into a visually polished application with a full theming system, 7 preset themes, and improved UX for identity sharing and clipboard.

**Architecture:** Theme struct with compiled-in presets + optional TOML overrides. Every UI component reads colors from the theme rather than hardcoding them.

---

## Theme System

### Theme Struct

A single `Theme` struct captures every color used across the UI, grouped by component:

```rust
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
```

### Config File

Location: `~/.config/chattor/theme.toml`

```toml
preset = "dark"  # dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn

[colors]
accent = "#00ffaa"
# Only specify overrides - everything else uses the preset
```

Colors support hex (`#00d4ff`), named (`cyan`, `red`), and 256-color indices.

### Theme Loading Order

1. Load preset from `--theme` CLI flag (if provided)
2. Else load `~/.config/chattor/theme.toml` (if exists)
3. Else use "dark" preset
4. Apply any TOML color overrides on top of preset

No hot-reload - change theme, restart app.

---

## Preset Themes

Seven built-in presets compiled as constants:

| Preset | Vibe | Background | Accent | Text |
|--------|------|-----------|--------|------|
| **dark** (default) | Deep blue-black, cyan glow | `#0a0e27` | `#00d4ff` | `#e0e0e0` |
| **light** | Clean white, blue accents | `#f5f5f5` | `#0066cc` | `#1a1a1a` |
| **cyberpunk** | Green-on-black Matrix | `#000000` | `#00ff41` | `#00ff41` |
| **minimal** | Pure monochrome, max contrast | `#000000` | `#ffffff` | `#cccccc` |
| **rose-pine** | Muted purple/pink warmth | `#191724` | `#ebbcba` | `#e0def4` |
| **rose-pine-moon** | Deeper variant | `#232136` | `#ea9a97` | `#e0def4` |
| **rose-pine-dawn** | Light warm variant | `#faf4ed` | `#d7827e` | `#575279` |

CLI usage: `chattor --theme cyberpunk`

---

## Visual Polish

### Layout & Borders

- Widen sidebar from 20 to 24 chars
- Use rounded border set (`╭╮╰╯`) instead of sharp corners
- Add 1-char padding inside conversation area
- Consistent title formatting with spaces inside borders

### Status Indicators

- Connection status in header: `◉ Connected` / `◌ Connecting...` with theme colors
- Message delivery icons apply theme colors (sent/delivered/read/failed)
- Keep `○` for friend online status (real status not yet available)

### Input Area

- Placeholder text when empty and focused: `Type a message...` in dim color
- Blinking cursor effect (alternate `█` and ` ` on animation tick)

### Footer

- Subtle separator line above footer
- Theme-colored keybinding hints: keys in accent color, descriptions in dim

---

## Share Info Redesign

### Remove Full-Screen Setup Wizard

New users see the same sidebar + conversation layout as everyone else, with an empty state message: `"Press [a] to add a friend, or [i] to view your identity"`

### Polish Identity Modal

- Clean layout with clear labels
- Copy buttons with visual feedback ("Copied!" flash)
- ASCII fingerprint visual hash for fun/verification
- Click-to-copy on header address too

### Header Address

Truncated onion address always visible in header bar (already partially implemented). Provides persistent reference without dominating the screen.

---

## Click-to-Copy Fix

- Debug and fix the existing `copy_to_clipboard` function using `arboard` crate
- Add visual feedback: briefly show "Copied!" indicator after successful copy
- Ensure clipboard works on both Linux (X11/Wayland) and macOS

---

## File Structure

### New Files

- `src/ui/theme.rs` - Theme struct, preset definitions, TOML parsing, hex color conversion
- `src/config/theme.rs` - Theme config loading from theme.toml

### Modified Files

- All `src/ui/*.rs` files - replace hardcoded colors with theme references
- `src/ui/app_ui.rs` - thread Theme through RenderContext, remove setup wizard
- `src/ui/conversation.rs` - empty state hint, rounded borders, padding
- `src/ui/sidebar.rs` - wider sidebar, rounded borders
- `src/ui/modals.rs` - polished identity modal with copy feedback
- `src/ui/mod.rs` - fix copy_to_clipboard, add visual feedback
- `src/cli.rs` - add --theme flag
- `src/config/settings.rs` - add theme field
- `src/main.rs` - load theme on startup, pass to render context
