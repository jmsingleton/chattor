# Ephemeral Messages Design

**Goal:** Self-destructing messages with configurable timer per conversation.

**Approach:** Per-conversation TTL setting, timer starts on read, cleanup each render cycle.

## Database Changes

**`conversations` table** — add column:
- `ephemeral_ttl INTEGER` — TTL in seconds (NULL = not ephemeral)

**`messages` table** — add columns:
- `expires_at INTEGER` — Unix timestamp for deletion (NULL = never)
- `ephemeral_ttl INTEGER` — Duration in seconds, stored so we know when to set expires_at on read

Both are ALTER TABLE ADD COLUMN with NULL defaults — no migration issues.

## Protocol

**PlaintextPayload** gets optional field:
- `ephemeral_ttl: Option<i64>` — tells receiver the deletion duration

Send path checks `conversations.ephemeral_ttl`, includes in payload if set.
Receive path stores `ephemeral_ttl` on the message row.

## Timer Logic

- Timer starts when message is **read** (conversation opened)
- `mark_conversation_read` sets `expires_at = now + ephemeral_ttl` for unset messages
- Cleanup query runs each render cycle: `DELETE FROM messages WHERE expires_at IS NOT NULL AND expires_at < now`

## UI

- **Conversation header**: `"Alice [⏱ 5m]"` when ephemeral active
- **Hotkey `e`**: Opens modal with presets: Off, 5 min, 1 hour, 24 hours, 7 days
- **Message rendering**: `⏱` prefix on ephemeral messages
- **Footer**: `[e] Ephemeral` hint when conversation selected

## Not Building

- Per-message ephemeral control
- Countdown timers on messages
- Syncing setting with peer
- Retroactive expiry on TTL change
