# Conversation UI & Text Messaging - Design Document

**Date:** 2026-02-10
**Status:** Approved
**Phase:** Phase 1 MVP completion

## Overview

Add the core chat interface to torrent-chat: a persistent 3-panel layout with friends sidebar, conversation view, message input, and full text message send/receive over Tor. Includes a guided setup wizard for new users and click-to-copy clipboard support.

**Goal:** Users can select a friend from the sidebar and exchange encrypted text messages in real-time over Tor.

**Architecture:** Persistent layout replaces modal-only UI. Messages encrypted via existing Signal Protocol sessions, sent via existing hybrid send (direct + queue fallback).

---

## Section 1: Layout Architecture

The app renders a persistent 3-panel layout at all times:

```
┌──────────────────────────────────────────────────────────┐
│  torrent-chat v0.1.0          [Tor: Connected]           │  ← Header (3 lines)
├────────────────┬─────────────────────────────────────────┤
│                │                                         │
│  Friends (2)   │  Bob                        2m ago      │
│  ▸ Bob      ●  │  ────────────────────────────────       │
│    Carol    ○  │  Hey, how's it going?                   │
│                │                                         │  ← Main (flexible)
│                │  You                        just now    │
│                │  ────────────────────────────────       │
│                │  Good! Working on torrent-chat           │
│                │                                         │
│                ├─────────────────────────────────────────┤
│                │  > Type a message...                    │  ← Input (3 lines)
├────────────────┴─────────────────────────────────────────┤
│  [Tab] Friends  [Enter] Send  [Esc] Nav  [q] Quit       │  ← Footer (1 line)
└──────────────────────────────────────────────────────────┘
```

- Sidebar: fixed at 20 characters wide
- Conversation panel: fills remaining width
- Modals (add friend, identity, friend request) overlay the center as they do today
- When friend list is empty, the main area shows a setup wizard instead

## Section 2: AppState Redesign

The `Normal` state becomes the persistent chat layout with conversation tracking:

```rust
pub enum AppState {
    Normal {
        selected_friend_idx: Option<usize>,
        conversation_id: Option<i64>,
        input: String,
        cursor: usize,
        input_focused: bool,
        scroll_offset: usize,
    },
    AddingFriend { input, cursor, error },
    ViewingFriendRequest { request_id, from_onion, friend_code, timestamp },
    ViewingMyIdentity { friend_code, onion_address },
}

pub enum AppAction {
    SelectFriend(usize),
    SendMessage(String),
    SendFriendRequest(String),
    AcceptFriendRequest(i64),
    RejectFriendRequest(i64),
    ViewMyIdentity,
    Quit,
}
```

- `input_focused: true` → keystrokes go to message input
- `input_focused: false` → keystrokes are navigation/shortcuts (Tab, a, i, q)
- `Escape` unfocuses input, `Enter` on a friend re-focuses it
- Setup wizard is a render path within Normal when friends list is empty (not a separate state)

## Section 3: Sidebar & Friend List

Each sidebar entry shows:
- Display name (or truncated .onion if no name set)
- Online indicator: `●` green / `○` gray
- Selection arrow: `▸` for currently selected friend
- Unread count badge: `(3)` if unread messages exist

Online status for MVP: green if we've sent/received a message to/from them in the last 5 minutes, gray otherwise. No active pinging.

Unread tracking via `last_read_at` column on `conversations` table. Messages with timestamp after `last_read_at` are unread. Selecting a conversation updates `last_read_at` to now.

Friend list query:
```sql
SELECT f.id, f.onion_address, f.display_name, f.status,
       c.id as conversation_id,
       (SELECT COUNT(*) FROM messages m
        WHERE m.conversation_id = c.id
        AND m.timestamp > COALESCE(c.last_read_at, 0)) as unread
FROM friends f
LEFT JOIN conversations c ON c.friend_id = f.id
WHERE f.status = 'active'
ORDER BY f.display_name, f.onion_address
```

Navigation: `Tab` moves focus to sidebar, `Up/Down` arrows move selection, `Enter` opens conversation and focuses input.

## Section 4: Conversation View & Message Display

Messages render with sender name (bold), content, and timestamp:

```
  Bob                              14:32
  ─────────────────────────────────────
  Hey, how's it going?

  You                              14:33
  ─────────────────────────────────────
  Good! Working on torrent-chat
```

- "You" for own messages, display name for friend's messages
- Relative time for recent ("just now", "2m ago"), absolute for older ("14:32", "Jan 5")
- Status indicators after own messages: `✓` sent, `⏳` queued, `✗` failed
- Load last 50 messages on conversation open, scroll up for more (paginated)
- `Up/Down` scroll when input unfocused, `Home/End` jump to oldest/newest

Empty states:
- No conversation selected: "Select a friend to start chatting" (centered, gray)
- Conversation with no messages: "No messages yet. Say hello!" (centered, gray)

## Section 5: Text Message Send/Receive Flow

**Sending (Enter with text in input):**
1. Look up or create conversation for selected friend
2. Generate UUID message_id
3. Store message locally in messages table (status: "sent")
4. Load Signal session, encrypt plaintext → TextMessage
5. Call try_send_direct() (5s timeout)
6. Success: status stays "sent", display ✓
7. Failure: enqueue via message_queue, status → "queued", display ⏳

**Receiving (TextMessage from listener):**
1. handle_incoming_message() routes TextMessage to new handler
2. Decrypt ciphertext using Signal session
3. Find or create conversation for sender
4. Store decrypted content in messages table
5. If conversation is open, next render picks it up (dirty flag)
6. If not viewing that conversation, unread count increments

**Dirty flag**: App gets a `messages_dirty: bool` field. Set true on send/receive. Render re-queries messages when dirty, then clears flag. Avoids polling database every frame.

## Section 6: Setup Wizard

When friend list is empty, the main area shows a guided setup:

```
┌──────────────────────────────────────────────────────────┐
│  torrent-chat v0.1.0          [Tor: Connected]           │
├──────────────────────────────────────────────────────────┤
│                                                          │
│              Welcome to torrent-chat                     │
│                                                          │
│   Step 1: Your identity                                  │
│   ┌────────────────────────────────────────────────┐     │
│   │  abc123...xyz.onion              [click to copy] │   │
│   │  happy-1234-tiger-5678           [click to copy] │   │
│   └────────────────────────────────────────────────┘     │
│                                                          │
│   Step 2: Share your .onion address with a friend        │
│                                                          │
│   Step 3: Press [a] to add their .onion address          │
│                                                          │
├──────────────────────────────────────────────────────────┤
│  [a] Add Friend  [i] My Identity  [q] Quit               │
└──────────────────────────────────────────────────────────┘
```

Not a separate AppState — just a render path within Normal when friend count is zero. All existing shortcuts (a, i, q) work. Disappears as soon as one active friend exists.

## Section 7: Database Changes

**Schema migration v4→v5**: Add `last_read_at` column to conversations:
```sql
ALTER TABLE conversations ADD COLUMN last_read_at INTEGER;
```

**New module `src/db/queries.rs`** with helper functions:
- `get_friends_with_unread()` → Vec<FriendEntry>
- `get_or_create_conversation(friend_id)` → conversation_id
- `get_messages(conversation_id, limit, offset)` → Vec<ChatMessage>
- `store_outgoing_message(conversation_id, sender_onion, content, message_id)`
- `store_incoming_message(conversation_id, sender_onion, content, message_id)`
- `mark_conversation_read(conversation_id)`
- `update_message_status(message_id, status)`

## Section 8: Clipboard & Mouse Support

**Mouse**: Enable crossterm mouse capture. Handle click events on .onion address and friend code in setup wizard and identity modal.

**Clipboard**: Add `arboard` crate for cross-platform clipboard access.

On click:
1. Copy text to system clipboard via `arboard::Clipboard`
2. Show "Copied!" feedback for 2 seconds (tracked via `copied_feedback: Option<Instant>` on state)

Clickable areas: setup wizard (.onion + friend code), identity modal (.onion + friend code).

Feature-gated if `arboard` causes platform issues.

## Success Criteria

1. App boots into chat layout with sidebar and conversation view
2. New users see setup wizard with copy-to-clipboard
3. Users can Tab to sidebar, arrow-key select a friend, Enter to open conversation
4. Users can type and send encrypted messages over Tor
5. Incoming messages appear in real-time in the conversation view
6. Unread counts show in sidebar for conversations not currently viewed
7. Message status indicators (sent/queued/failed) display correctly
8. All existing functionality (add friend, accept/reject request, identity modal) still works
