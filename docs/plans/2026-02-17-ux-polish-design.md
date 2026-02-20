# UX Polish Design: Typing Indicators, Online Status, Desktop Notifications

**Date:** 2026-02-17
**Status:** Approved

## Overview

Add three UX polish features to chattor via a unified presence system. Typing indicators and online status share a single protocol message type and in-memory state layer. Desktop notifications hook into the existing incoming message handler.

## Design Principles

- **Privacy-first**: No "last seen" persistence, no message content in notifications
- **Ephemeral by nature**: Presence state lives in memory only, resets on restart
- **Lightweight**: Presence messages are unencrypted (Tor provides transport security), avoiding unnecessary Signal ratchet advances
- **Simple**: Global notification toggle, no per-friend settings

## Protocol Changes

One new variant added to the `Message` enum (13 total):

```rust
Presence(PresenceMessage)

pub struct PresenceMessage {
    pub from_onion: String,
    pub presence_type: PresenceType,
    pub timestamp: i64,
}

pub enum PresenceType {
    Heartbeat,       // "I'm online"
    TypingStarted,   // "I started typing"
    TypingStopped,   // "I stopped typing"
}
```

**Key decisions:**
- Not encrypted — ephemeral metadata, not content. Tor circuit provides transport encryption. Encrypting would burn a Signal ratchet step per heartbeat (every 60s).
- No `to_onion` field — sent over direct peer connection, recipient is implicit.
- Explicit `TypingStopped` — sender signals when input is cleared or message is sent. 5s timeout as fallback.

## Presence State

### In-Memory State

```rust
pub struct PeerPresence {
    pub is_online: bool,
    pub is_typing: bool,
    pub last_seen: Instant,
    pub typing_started: Option<Instant>,
}
```

Stored in `HashMap<String, PeerPresence>` on the `App` struct, keyed by onion address. Not persisted to DB.

### Background Task: Outgoing Heartbeats

- Spawned alongside queue processor and Tor listener
- Every 60 seconds, send `Presence::Heartbeat` to friends with existing cached connections in the pool
- Do NOT build new Tor circuits just for heartbeats — skip peers without active connections

### Incoming Presence Handling

- `Heartbeat` -> set `is_online = true`, update `last_seen`
- `TypingStarted` -> set `is_typing = true`, record `typing_started`
- `TypingStopped` -> set `is_typing = false`, clear `typing_started`

### Timeouts

- **Offline threshold: 2 minutes** — no heartbeat in 2min = offline. Checked lazily during sidebar render.
- **Typing timeout: 5 seconds** — `typing_started` older than 5s = stopped. Checked lazily during render.

### Outgoing Typing Detection

- On first keypress into input (empty -> non-empty, or after 4s silence): send `TypingStarted`
- On send message or clear input: send `TypingStopped`
- Debounce: no duplicate `TypingStarted` within 4 seconds

## UI Changes

### Sidebar Status Icons

Replace hardcoded `○` with dynamic icons:

| State | Icon | Color |
|-------|------|-------|
| Online | `●` | Theme success/green |
| Typing | `✎` | Theme accent |
| Offline | `○` | Theme muted/dim |

Priority: Typing > Online > Offline.

### Conversation Typing Indicator

When selected friend `is_typing`, render below the last message:

```
Alice is typing…
```

Styled in theme's muted/dim color. Sits between message list and input box.

### Desktop Notifications

Using `notify-rust` crate:

- **Trigger**: incoming `TextMessage` when terminal is not focused
- **Content**: `"New message from {display_name}"` — never include message content
- **Focus detection**: best-effort via platform APIs, fall back to always notifying
- **Global toggle**: `app_settings` key `notifications_enabled`, defaults to `"true"`
- **Settings UI**: `[n]` keybinding in Normal state toggles on/off, footer flash confirmation
- **No sound**: TUI apps shouldn't beep

## Database Changes

No schema migration needed. Uses existing `app_settings` key-value table (v8):
- Key: `notifications_enabled`, Value: `"true"` (default)

No presence persistence — deliberate privacy decision.

## Testing Strategy

### Unit Tests
- `PresenceMessage` serialization/deserialization round-trip
- `PeerPresence` timeout logic (online->offline at 2min, typing->stopped at 5s)
- Typing debounce (no duplicate `TypingStarted` within 4s)
- Notification toggle in `app_settings`

### Integration Tests
- Heartbeat -> `PeerPresence` update
- TypingStarted -> is_typing=true -> 5s timeout -> is_typing=false
- Full flow: type in input -> TypingStarted sent -> send message -> TypingStopped sent

### E2E Tests
- Two peers establish session, exchange heartbeats, verify mutual online status
- Typing round-trip: Alice types -> Bob sees typing -> Alice sends -> Bob sees message

### Not Tested
- Desktop notification display (platform-dependent)
- Terminal focus detection (environment-specific)
