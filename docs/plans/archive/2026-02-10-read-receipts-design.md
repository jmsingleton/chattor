# Read Receipts Design

**Goal:** Wire up delivery and read receipts so senders see message status progression.

**Approach:** Reuse existing DeliveryReceiptMessage struct, add ReadReceipt variant, queue receipts through existing message queue.

## Protocol

**New Message variant:** `ReadReceipt(DeliveryReceiptMessage)` — same struct, different wire tag (`"read_receipt"`).

**Status progression:** `sent` → `delivered` → `read`

## Send & Receive Flow

**Delivery receipt:** Receive TextMessage → immediately queue DeliveryReceipt back to sender. Sender receives → updates message status to `delivered`.

**Read receipt:** Open conversation (SelectFriend/mark_conversation_read) → query messages from peer with status `received` → queue ReadReceipt for each. Sender receives → updates message status to `read`.

Receipts go through existing message queue for offline resilience.

## Database

No schema changes. Status column already supports string values. Add query to find messages needing read receipts:
```sql
SELECT message_id, sender_onion FROM messages
WHERE conversation_id = ?1 AND status = 'received' AND sender_onion != ?2
```

## UI

Status indicators on outgoing messages:
- `sent` → `✓` (gray)
- `queued` → `⏳`
- `failed` → `✗`
- `delivered` → `✓✓` (gray)
- `read` → `✓✓` (green)

## Not Building

- Per-message read tracking (conversation-level sufficient)
- Privacy toggle to disable read receipts
- Typing indicators
