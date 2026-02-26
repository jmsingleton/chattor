---
title: Daemon, CLI & MCP
description: Run chattor as a headless daemon with CLI commands and AI agent integration
---

## Overview

chattor can run in two modes:

- **TUI mode** (default): The interactive terminal UI
- **Daemon mode**: A headless background process that exposes a JSON-RPC API over a Unix socket

The daemon enables CLI scripting, automation, and AI agent integration via MCP (Model Context Protocol).

## Starting the Daemon

```bash
chattor daemon
```

The daemon:
- Bootstraps Tor and starts a hidden service (same as TUI mode)
- Opens a Unix socket at `{data_dir}/chattor.sock`
- Writes a PID file at `{data_dir}/chattor.pid` for mutual exclusion
- Runs background tasks (message queue, channel sync, heartbeat)
- Processes incoming messages from peers

Only one daemon instance can run at a time. If a daemon is already running, `chattor daemon` will exit with an error.

## CLI Commands

With a daemon running, use CLI subcommands to interact:

### Identity & Status

```bash
chattor status              # Daemon status + Tor connection info
chattor identity            # Your onion address and friend code
```

### Friends

```bash
chattor friends list                            # List all friends
chattor friends add <friend-code>               # Send a friend request
chattor friends requests                        # Show pending requests
chattor friends accept <friend-code>            # Accept a request
chattor friends reject <friend-code>            # Reject a request
```

### Messaging

```bash
chattor send <onion-address> "Hello!"           # Send a message
chattor recv                                    # Fetch recent messages
chattor recv --conversation <onion> --limit 50  # Filter by conversation
chattor listen                                  # Stream incoming messages (real-time)
```

The `listen` command streams messages as newline-delimited JSON until interrupted with Ctrl+C. Useful for building bots or notification integrations.

### Channels

```bash
chattor channels list                           # List your channels + subscriptions
chattor channels publish <channel-id> "Post"    # Publish to a channel
chattor channels subscribe <onion>              # Subscribe to a peer's channel
chattor channels feed <channel-id>              # View recent posts
```

### Settings

```bash
chattor ephemeral <seconds>     # Set ephemeral message TTL (0 to disable)
chattor notifications on        # Enable desktop notifications
chattor notifications off       # Disable desktop notifications
```

## MCP Server

chattor includes an MCP (Model Context Protocol) server for AI agent integration. This lets Claude and other MCP-compatible agents send and receive messages through chattor.

### Setup

Add chattor to your MCP client configuration. For Claude Code, add to `~/.claude.json`:

```json
{
  "mcpServers": {
    "chattor": {
      "command": "chattor",
      "args": ["mcp"]
    }
  }
}
```

The MCP server requires a running daemon — start one with `chattor daemon` before using MCP tools.

### Available Tools

| Tool | Description |
|------|-------------|
| `get_status` | Daemon and Tor connection status |
| `get_identity` | Your onion address and friend code |
| `list_friends` | All friends with online status |
| `add_friend` | Send a friend request |
| `accept_friend_request` | Accept a pending request |
| `send_message` | Send a message to a friend |
| `receive_messages` | Fetch recent messages |
| `publish_channel_post` | Publish to a broadcast channel |
| `list_channel_posts` | View posts from a channel |

### Example Agent Interaction

Once configured, an AI agent can interact naturally:

> "Send a message to alice saying I'll be online at 8pm"

The agent calls `send_message` with Alice's onion address and the text, routed through the daemon over Tor with full Signal Protocol encryption.

## Architecture

```
┌─────────┐  ┌──────┐  ┌──────────┐
│ CLI     │  │ MCP  │  │ Scripts  │
│ commands│  │server│  │ & bots   │
└────┬────┘  └──┬───┘  └────┬─────┘
     │          │            │
     └────┬─────┴────────────┘
          │ JSON-RPC 2.0
          │ Unix socket
     ┌────▼─────────────────┐
     │    chattor daemon    │
     │                      │
     │  Tor ←→ Signal Proto │
     │  Message Queue       │
     │  Connection Pool     │
     │  Presence System     │
     └──────────────────────┘
```

The daemon owns the `App` state (database, Tor client, Signal sessions) behind an `Arc<Mutex<App>>`. The CLI and MCP server are thin clients that connect to the daemon's Unix socket and issue JSON-RPC calls.

## Data Paths

| Platform | Socket | PID File |
|----------|--------|----------|
| **Linux** | `~/.local/share/chattor/chattor.sock` | `~/.local/share/chattor/chattor.pid` |
| **macOS** | `~/Library/Application Support/chattor/chattor.sock` | `~/Library/Application Support/chattor/chattor.pid` |
