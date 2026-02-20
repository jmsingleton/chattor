---
title: Theming
description: Customize chattor's appearance with built-in themes or your own colors
---

## Built-in Themes

chattor ships with 7 themes:

| Theme | Description |
|-------|-------------|
| `dark` | Default dark theme |
| `light` | Light background |
| `cyberpunk` | Neon accents on dark |
| `minimal` | Subdued, low-contrast |
| `rose-pine` | Rosé Pine dark |
| `rose-pine-moon` | Rosé Pine Moon (darker) |
| `rose-pine-dawn` | Rosé Pine Dawn (light) |

Switch themes via CLI:

```bash
chattor --theme cyberpunk
```

## Custom Theme via TOML

Create `~/.config/chattor/theme.toml` to override any color:

```toml
# Start from a preset
name = "my-custom"

# Override specific colors (hex format)
bg = "#1a1b26"
fg = "#c0caf5"
accent = "#7aa2f7"
border = "#3b4261"
border_focused = "#7aa2f7"

# Component-specific overrides
sidebar_selected_fg = "#7aa2f7"
sidebar_unread = "#e0af68"
msg_own_sender = "#9ece6a"
msg_peer_sender = "#7aa2f7"
error = "#f7768e"
success = "#9ece6a"
```

## Available Color Fields

Every field in the theme is overridable:

**Global:** `bg`, `fg`, `fg_dim`, `accent`, `border`, `border_focused`

**Header:** `header_fg`, `header_accent`

**Sidebar:** `sidebar_selected_fg`, `sidebar_unread`, `sidebar_status_online`, `sidebar_channel_header`

**Conversation:** `msg_own_sender`, `msg_peer_sender`, `msg_timestamp`, `msg_status_sent`, `msg_status_delivered`, `msg_status_read`, `msg_status_failed`, `msg_ephemeral`

**Input:** `input_fg`, `input_placeholder`

**Modals:** `modal_border`, `modal_title`, `error`, `warning`, `success`

**Channels:** `channel_border`, `channel_read_count`

## Config Paths

| Platform | Path |
|----------|------|
| **Linux** | `~/.config/chattor/theme.toml` |
| **macOS** | `~/Library/Application Support/chattor/theme.toml` |
