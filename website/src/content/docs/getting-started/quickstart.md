---
title: Quickstart
description: Get up and running with chattor in 5 minutes
---

## First Run

Launch chattor:

```bash
chattor
```

Or with a theme:

```bash
chattor --theme rose-pine
```

On first run, chattor will:

1. Generate your **Ed25519 identity keypair**
2. Create an encrypted **SQLCipher database**
3. Bootstrap an embedded **Tor connection** via arti
4. Launch your **Tor hidden service** (your .onion address)

The bootstrap takes 30-60 seconds while Tor establishes circuits. You'll see an animated loading screen.

## CLI Options

| Flag | Short | Description |
|------|-------|-------------|
| `--debug` | `-d` | Enable debug logging |
| `--theme <name>` | `-t` | Theme preset (see [Theming](/guides/theming/)) |
| `--config-dir <path>` | `-c` | Custom config directory |

## Adding a Friend

1. Press **`[a]`** to open the Add Friend modal
2. Enter your friend's **32-word friend code** (they can find it by pressing **`[i]`** for Identity)
3. A friend request is sent over Tor
4. Once they accept, you can exchange messages

## Your Identity

Press **`[i]`** to view your identity:

- Your **.onion address** — this is your network identity
- Your **friend code** — share this with people who want to reach you
- Press **`[o]`** to copy your onion address, **`[c]`** to copy your friend code

## Sending Messages

1. Select a friend in the sidebar (arrow keys or mouse)
2. Type your message in the input bar
3. Press **Enter** to send

Messages are encrypted with Signal Protocol before leaving your machine. If your friend is offline, messages are queued and delivered automatically when they come back online.

## Keybindings

| Key | Action |
|-----|--------|
| `q` | Quit |
| `a` | Add friend |
| `i` | View identity |
| `r` | View friend requests |
| `n` | Toggle notifications |
| `Tab` | Switch between sidebar and chat |
| `↑/↓` | Navigate friends/messages |
