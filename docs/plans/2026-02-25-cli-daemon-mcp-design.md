# CLI, Daemon & MCP Server Design

**Date:** 2026-02-25
**Status:** Approved

## Overview

Add a headless daemon mode and CLI interface to chattor, enabling programmatic agent-to-agent messaging over Tor with E2E encryption. Also expose chattor as an MCP (Model Context Protocol) server so LLM agents can use it as a native tool.

### Value Proposition

chattor provides private, decentralized, E2E-encrypted peer-to-peer messaging with no central server. Wrapping this in a CLI/MCP interface gives AI agents a communication channel where:

- No one can read the messages (Signal Protocol E2E encryption)
- No central server logs metadata or routes traffic (pure P2P over Tor)
- No account signup, API keys, or third-party dependencies
- Messages are authenticated (Ed25519 signing)

No existing agent communication tool offers this combination.

---

## Architecture

Two mutually exclusive runtime modes:

```
Mode 1: chattor tui          (standalone, as today, unchanged)
Mode 2: chattor daemon   +   { CLI subcommands, MCP server }

┌─────────┐  ┌──────────────┐
│   CLI   │  │  MCP Server  │   ← Thin clients
└────┬────┘  └──────┬───────┘
     │  Unix socket  │
     └───────┬───────┘
       ┌─────┴─────┐
       │   Daemon   │   ← Tor, crypto, DB, queue, presence
       └───────────┘
```

### Daemon (`chattor daemon`)

Headless process that owns all runtime state:

- Tor client + hidden service
- Signal Protocol sessions
- SQLite database (WAL mode)
- Message queue with retry
- Connection pool
- Presence/heartbeat
- Incoming message listener

Listens on a Unix socket at `{data_dir}/chattor.sock` for JSON-RPC 2.0 commands.

### CLI (`chattor <subcommand>`)

Thin client. Connects to daemon socket, sends a JSON-RPC request, prints the JSON response to stdout, exits. No Tor, no DB, no crypto — just socket I/O and argument parsing.

### MCP Server (`chattor mcp`)

Launched by agent frameworks as a subprocess. Communicates with agents via stdio (stdin/stdout JSON-RPC, the standard MCP transport). Internally connects to the daemon socket and translates MCP tool calls to daemon commands.

### TUI (`chattor tui`)

The existing TUI, unchanged. Runs standalone with its own Tor connection. Mutually exclusive with the daemon (only one process can own the Tor hidden service at a time).

### Mutual Exclusion

The daemon creates a PID file at `{data_dir}/chattor.pid`. Both daemon and TUI check for an existing PID file on startup and refuse to start if another instance is running. The PID file is cleaned up on graceful shutdown.

---

## CLI Subcommands

All commands output JSON to stdout. Errors are `{"error": "message", "code": "ERROR_CODE"}`.

### Daemon Management

```
chattor daemon                   # Start daemon (foreground)
chattor daemon --background      # Daemonize (fork to background)
chattor daemon stop              # Stop running daemon
chattor status                   # Daemon status, Tor status, onion address
```

### Identity

```
chattor identity                 # Show friend code + .onion address
```

### Friends

```
chattor friends list             # List friends with online/typing status
chattor friends add <code>       # Send friend request (onion or friend code)
chattor friends remove <onion>   # Remove friend
chattor friends requests         # List pending incoming friend requests
chattor friends accept <id>      # Accept friend request
chattor friends reject <id>      # Reject friend request
```

### Messaging

```
chattor send <peer> <message>    # Send message to peer
chattor recv [--peer <peer>]     # Poll unread messages (optionally filtered)
chattor listen                   # Stream new messages as JSON lines (blocking)
```

### Channels

```
chattor channels list            # List own channels + subscriptions
chattor channels publish <type> <msg>  # Post to channel (public/friends)
chattor channels subscribe <onion>     # Subscribe to peer's channel
chattor channels feed [--channel <id>] # Read channel posts
```

### Settings

```
chattor ephemeral set <peer> <ttl>     # Set ephemeral TTL for conversation
chattor notifications <on|off>         # Toggle desktop notifications
```

### MCP

```
chattor mcp                      # Start MCP server (stdio transport)
```

---

## IPC Protocol

JSON-RPC 2.0 over Unix domain socket (`{data_dir}/chattor.sock`).

### Request Format

```json
{"jsonrpc": "2.0", "id": 1, "method": "send_message", "params": {"peer": "abc.onion", "message": "hello"}}
```

### Response Format

```json
{"jsonrpc": "2.0", "id": 1, "result": {"status": "sent", "message_id": "msg-123"}}
```

### Error Format

```json
{"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "Tor not connected"}}
```

### Streaming (listen)

For the `listen` method, the daemon sends newline-delimited JSON events on the socket as messages arrive. The connection stays open until the client disconnects.

```json
{"jsonrpc": "2.0", "method": "message_received", "params": {"from": "xyz.onion", "message": "hello", "timestamp": 1740500000}}
```

### Methods

| Method | Params | Returns |
|--------|--------|---------|
| `status` | — | `{daemon: bool, tor: bool, onion: string}` |
| `identity` | — | `{friend_code: string, onion: string}` |
| `friends_list` | — | `[{name, onion, online, typing, unread_count}]` |
| `friends_add` | `{code: string}` | `{status: "sent"\|"queued"}` |
| `friends_remove` | `{onion: string}` | `{status: "removed"}` |
| `friends_requests` | — | `[{id, from_onion, friend_code, received_at}]` |
| `friends_accept` | `{id: int}` | `{status: "accepted"}` |
| `friends_reject` | `{id: int}` | `{status: "rejected"}` |
| `send_message` | `{peer: string, message: string}` | `{status: "sent"\|"queued", message_id: string}` |
| `recv_messages` | `{peer?: string, since?: int}` | `[{id, from, message, timestamp, read}]` |
| `listen` | — | streaming (see above) |
| `channels_list` | — | `[{id, type, publisher, post_count}]` |
| `channels_publish` | `{channel_type: string, message: string}` | `{post_id: string}` |
| `channels_subscribe` | `{onion: string}` | `{status: "subscribed"}` |
| `channels_feed` | `{channel_id?: int}` | `[{post_id, author, message, timestamp, read_count}]` |
| `ephemeral_set` | `{peer: string, ttl: int\|null}` | `{status: "set"}` |
| `notifications_toggle` | `{enabled: bool}` | `{enabled: bool}` |

---

## MCP Tools

The MCP server exposes these tools (mapping to daemon methods):

| Tool | Description | Maps to |
|------|-------------|---------|
| `send_message` | Send a private message to a peer | `send_message` |
| `receive_messages` | Get unread messages, optionally filtered by peer | `recv_messages` |
| `list_friends` | List friends with online/typing status | `friends_list` |
| `add_friend` | Send a friend request via onion address or friend code | `friends_add` |
| `accept_friend_request` | Accept a pending friend request | `friends_accept` |
| `get_identity` | Get own friend code and onion address | `identity` |
| `get_status` | Check daemon and Tor connection status | `status` |
| `publish_channel_post` | Post a message to own channel | `channels_publish` |
| `list_channel_posts` | Read posts from a channel | `channels_feed` |

MCP transport: stdio (JSON-RPC over stdin/stdout). This is the universally supported local MCP transport used by Claude Code, Cursor, and other agent frameworks.

---

## Daemon Lifecycle

### Startup

1. Check for existing PID file — refuse to start if another instance is running
2. Write PID file to `{data_dir}/chattor.pid`
3. Open database, initialize schema
4. Bootstrap Tor client and launch hidden service
5. Start background tasks: message queue processor, heartbeat, channel sync, incoming listener
6. Bind Unix socket at `{data_dir}/chattor.sock`
7. Accept connections and process JSON-RPC requests

### Shutdown

1. SIGTERM/SIGINT received
2. Close Unix socket (stop accepting new connections)
3. Drain in-flight requests
4. Stop background tasks
5. Close Tor connection
6. Remove PID file and socket file
7. Exit

### Background mode

`chattor daemon --background` forks to background, redirects stdout/stderr to `{data_dir}/chattor.log`, and exits the parent process.

---

## Known Tradeoffs

- **Tor latency:** 2-5 seconds per message round trip. Suitable for async agent workflows, not for sub-second real-time coordination.
- **Bootstrap time:** 30-60 seconds for initial Tor bootstrap. Subsequent startups with cached state are faster (~10-15s).
- **Setup friction:** Agents must install chattor, run the daemon, exchange friend codes, and accept requests before messaging. The privacy guarantee is the tradeoff.
- **One identity per daemon:** All messages from one daemon share the same .onion address. Multiple agents on one machine need separate data directories and daemon instances.
- **TUI/daemon mutual exclusion:** Only one can run at a time. The TUI is not refactored to be a daemon client in v1.

---

## Testing Strategy

- **Unit tests:** JSON-RPC request/response serialization, CLI argument parsing, method dispatch
- **Integration tests:** Daemon lifecycle (start → socket connect → request → response → stop), PID file locking
- **MCP tests:** Tool call → daemon request → response → MCP result mapping
- **E2E tests:** Two daemons exchanging messages (build on existing e2e test infrastructure)
