# Phase 3: Broadcast Channels Design

**Date:** 2026-02-12
**Status:** Approved

## Overview

Add one-to-many broadcast channels to chattor. Each user automatically gets two channels: a Public channel (anyone can subscribe via .onion address) and a Friends-Only channel (auto-subscribed when a friend is added). Channels are strictly one-way — publisher posts, subscribers read — with read receipt counts visible to the publisher.

## Architecture

Hybrid approach: extend the existing message protocol with new message variants. Public channel posts are signed with Ed25519 (readable by any subscriber, verifiable). Friends-only channel posts are encrypted via existing Signal sessions.

## Data Model

Schema v7 migration adds five tables:

```sql
-- Each user has exactly 2 channels (created on first launch)
channels (
  id INTEGER PRIMARY KEY,
  channel_type TEXT NOT NULL,        -- 'public' or 'friends_only'
  created_at INTEGER NOT NULL
)

-- Posts published to a channel
channel_posts (
  id INTEGER PRIMARY KEY,
  channel_id INTEGER NOT NULL,       -- FK to channels
  content TEXT NOT NULL,             -- markdown text
  post_id TEXT NOT NULL UNIQUE,      -- UUID for dedup
  created_at INTEGER NOT NULL,
  signature TEXT NOT NULL,           -- Ed25519 signature of content
  FOREIGN KEY (channel_id) REFERENCES channels(id)
)

-- Publisher side: who is subscribed to my channels
channel_subscribers (
  id INTEGER PRIMARY KEY,
  subscriber_onion TEXT NOT NULL,
  channel_type TEXT NOT NULL,
  subscribed_at INTEGER NOT NULL,
  UNIQUE(subscriber_onion, channel_type)
)

-- Subscriptions to remote users' channels
channel_subscriptions (
  id INTEGER PRIMARY KEY,
  publisher_onion TEXT NOT NULL,     -- whose channel
  channel_type TEXT NOT NULL,        -- 'public' or 'friends_only'
  subscribed_at INTEGER NOT NULL,
  last_sync_at INTEGER,             -- for pull-based sync
  UNIQUE(publisher_onion, channel_type)
)

-- Read receipts for posts (publisher side)
channel_post_receipts (
  id INTEGER PRIMARY KEY,
  post_id TEXT NOT NULL,
  reader_onion TEXT NOT NULL,
  read_at INTEGER NOT NULL,
  UNIQUE(post_id, reader_onion)
)
```

**Retention:** Fixed at last 100 posts per channel. Older posts auto-deleted after insert.

## Protocol Messages

Six new `Message` variants:

| Message | Direction | Purpose |
|---------|-----------|---------|
| `ChannelSubscribe` | subscriber → publisher | Request to subscribe (includes channel_type) |
| `ChannelUnsubscribe` | subscriber → publisher | Unsubscribe from channel |
| `ChannelPost` | publisher → subscriber | A new post (pushed to online subscribers) |
| `ChannelSyncRequest` | subscriber → publisher | "Give me posts since timestamp X" |
| `ChannelSyncResponse` | publisher → subscriber | Batch of missed posts |
| `ChannelPostReceipt` | subscriber → publisher | "I read post X" |

### Encryption Rules

- **Public channel** messages: Content is plaintext, signed with publisher's Ed25519 key. Any subscriber can verify authenticity without a Signal session.
- **Friends-only channel** messages: Encrypted via existing Signal sessions (subscriber is already a friend).
- `ChannelSubscribe` for public channels: Sent unencrypted (no session exists). Contains subscriber's .onion so publisher knows where to push.
- `ChannelSubscribe` for friends-only: Sent via Signal session (subscriber is already a friend).

### Post Structure

```
ChannelPost {
  publisher_onion: String,
  channel_type: "public" | "friends_only",
  post_id: UUID,
  content: String,           // markdown
  created_at: i64,
  signature: String,         // Ed25519 sig of (post_id + content + created_at)
}
```

## Data Flow

### Publishing a Post

1. User composes post in channel UI, selects which channel (public or friends-only)
2. Post stored locally in `channel_posts` with Ed25519 signature
3. Publisher looks up all subscribers for that channel from `channel_subscribers`
4. For online subscribers: push `ChannelPost` immediately via Tor
5. Offline subscribers pull when they come online

### Subscribing

1. **Friends-only (auto):** When a friend request is accepted, both sides automatically subscribe to each other's friends-only channel. No extra UI step.
2. **Public (manual via .onion):** User enters a .onion address and sends `ChannelSubscribe`. Publisher stores the subscriber's .onion.
3. **Public (auto from friends):** When a friend is added, their public channel also auto-appears in the sidebar.

### Subscriber Sync (Pull-Based)

1. On startup, subscriber connects to each publisher's hidden service
2. Sends `ChannelSyncRequest` with `last_sync_at` timestamp
3. Publisher responds with `ChannelSyncResponse` containing missed posts (up to 100)
4. Subscriber stores posts locally and updates `last_sync_at`

### Read Receipts

1. When subscriber views a post, sends `ChannelPostReceipt` to publisher
2. Publisher stores in `channel_post_receipts`
3. Publisher UI shows read count per post (e.g., "seen by 5")

## UI

### Sidebar

- New collapsible "Channels" section below the friends list
- Two sub-sections: "My Channels" (your public + friends-only) and "Subscriptions" (others' channels)
- Each channel shows: publisher name/onion, channel type icon, unread post count
- Friends-only channels appear automatically when a friend is added

### Channel Feed View

Replaces conversation view when a channel is selected:

- Reverse chronological list of posts, **newest at top**, scrolling down for older
- Each post shows: content (basic markdown rendered), timestamp, signature verification indicator
- For your own channels: read count per post (e.g., "seen by 5")
- No message input bar when viewing someone else's channel (one-way)

### Compose View

When viewing your own channel:

- Text input at bottom, same position as message input
- Keybinding to switch between Public and Friends-Only channel
- Post preview with basic markdown rendering

### Keyboard Navigation

- Existing friend list navigation extends to channel section
- Tab or arrow keys to move between friends and channels sections
- Same key to enter a channel as to enter a conversation

## Error Handling & Edge Cases

- **Subscriber connects to offline publisher:** Sync request fails silently. Retry on next startup or periodic background check (every 5 minutes).
- **Publisher receives subscribe from blocked .onion:** Reject silently. Reuse existing `blocked_onions` table.
- **Duplicate posts:** `post_id` UUID + UNIQUE constraint. `INSERT OR IGNORE` on subscriber side.
- **Retention enforcement:** After inserting a new post, delete oldest posts if count exceeds 100 per channel. Runs on both publisher and subscriber side.
- **Public channel subscribe without Signal session:** `ChannelSubscribe` sent unsigned/unencrypted. Acceptable since public channels are public by definition.
- **Friend removed:** Auto-unsubscribe from their friends-only channel. Remove their posts from local DB. Public channel subscription stays unless manually unsubscribed.
- **Signature verification fails:** Discard the post. Log a warning.
