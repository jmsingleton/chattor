# Database Migration Fix

## Problem

If you get this error:
```
Error: Database("Failed to create tables: no such column: message_id...")
```

This happens because:
- Your database was created with Phase 1 schema (no `message_id` column in messages table)
- Phase 2 schema expects `message_id` to exist
- The index creation fails on a non-existent column

## Quick Fix

Delete the old database and start fresh:

**On macOS:**
```bash
rm -rf ~/Library/Application\ Support/torrent-chat/
cargo run
```

**On Linux:**
```bash
rm -rf ~/.local/share/torrent-chat/
cargo run
```

The app will create a new database with the complete Phase 2 schema.

## Proper Migration (Future Enhancement)

For production, we should implement proper schema migrations:

```rust
// Check schema version
let current_version = get_schema_version()?;

if current_version < 2 {
    // Migrate from v1 to v2
    conn.execute("ALTER TABLE messages ADD COLUMN message_id TEXT UNIQUE", [])?;
    // ... other migrations
    set_schema_version(2)?;
}
```

This would be added to `src/db/connection.rs` in the `Database::new()` method.

## What Phase 2 Added to Messages Table

```sql
-- Phase 1: messages table had no message_id
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL,
    sender_onion TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'sent'
);

-- Phase 2: Added message_id for deduplication
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT UNIQUE NOT NULL,  -- NEW: UUID for deduplication
    conversation_id INTEGER NOT NULL,
    sender_onion TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'sent'
);
```

The `message_id` is used for:
- Deduplication (prevent duplicate delivery)
- Delivery receipt matching
- Message tracking across retries
