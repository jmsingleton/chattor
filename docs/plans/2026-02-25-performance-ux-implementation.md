# Performance & UX Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all crash bugs, eliminate critical performance bottlenecks, and bring input/navigation/visual UX up to terminal-native expectations.

**Architecture:** Four sequential tracts: (1) crash prevention & error surfacing, (2) render loop & mutex restructuring, (3) input & navigation polish, (4) visual & feedback polish. Tracts 1 and 2 are independent; Tract 3 builds on Tract 1's UTF-8 cursor fix; Tract 4 is independent.

**Tech Stack:** Rust, ratatui, tokio, rusqlite, DashMap, crossterm

---

## Tract 1: Crash Prevention & Error Surfacing

### Task 1.1: Fix UTF-8 Cursor Tracking in Text Input

The `cursor` field tracks byte position but `String::insert(byte_pos, char)` panics when `byte_pos` falls mid-character. We need to extract a shared helper that converts between char index and byte position, then use char-index for all cursor arithmetic.

**Files:**
- Create: `src/ui/input.rs` (shared input editing helpers)
- Modify: `src/ui/state.rs:107-147` (Normal input handling)
- Modify: `src/ui/state.rs:214-253` (AddingFriend input)
- Modify: `src/ui/state.rs:391-431` (ViewingChannel input)
- Modify: `src/ui/state.rs:434-461` (SubscribingToChannel input)
- Modify: `src/ui/conversation.rs:200-212` (render_input cursor slicing)
- Modify: `src/ui/mod.rs` (add `pub mod input;`)

**Step 1: Write failing tests for the helper**

Add to a new file `src/ui/input.rs`:

```rust
/// Shared text input editing functions.
/// All cursor positions are in **char indices** (not byte offsets).
/// Callers store `cursor: usize` as a char index.

/// Insert a character at the char-index cursor position.
pub fn insert_char(input: &mut String, cursor: &mut usize, c: char) {
    let byte_pos = char_to_byte(input, *cursor);
    input.insert(byte_pos, c);
    *cursor += 1;
}

/// Delete the character before the cursor (backspace).
pub fn backspace(input: &mut String, cursor: &mut usize) {
    if *cursor > 0 {
        *cursor -= 1;
        let byte_pos = char_to_byte(input, *cursor);
        input.remove(byte_pos);
    }
}

/// Move cursor left by one char.
pub fn move_left(cursor: &mut usize) {
    if *cursor > 0 {
        *cursor -= 1;
    }
}

/// Move cursor right by one char.
pub fn move_right(input: &str, cursor: &mut usize) {
    if *cursor < input.chars().count() {
        *cursor += 1;
    }
}

/// Convert char index to byte offset. Clamps to end of string.
pub fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte, _)| byte)
        .unwrap_or(s.len())
}

/// Split a string at a char index into (before, after) for rendering.
pub fn split_at_char(s: &str, char_idx: usize) -> (&str, &str) {
    let byte = char_to_byte(s, char_idx);
    (&s[..byte], &s[byte..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_insert_and_backspace() {
        let mut s = String::new();
        let mut c = 0;
        insert_char(&mut s, &mut c, 'h');
        insert_char(&mut s, &mut c, 'i');
        assert_eq!(s, "hi");
        assert_eq!(c, 2);
        backspace(&mut s, &mut c);
        assert_eq!(s, "h");
        assert_eq!(c, 1);
    }

    #[test]
    fn emoji_insert() {
        let mut s = String::new();
        let mut c = 0;
        insert_char(&mut s, &mut c, '😀');
        insert_char(&mut s, &mut c, 'a');
        assert_eq!(s, "😀a");
        assert_eq!(c, 2);
        // Cursor at char 2 = after 'a'
        assert_eq!(char_to_byte(&s, 2), 5); // 😀 is 4 bytes + 'a' is 1
    }

    #[test]
    fn emoji_backspace() {
        let mut s = "😀a".to_string();
        let mut c = 2;
        backspace(&mut s, &mut c); // delete 'a'
        assert_eq!(s, "😀");
        assert_eq!(c, 1);
        backspace(&mut s, &mut c); // delete 😀
        assert_eq!(s, "");
        assert_eq!(c, 0);
    }

    #[test]
    fn cjk_insert_mid_string() {
        let mut s = "你好".to_string();
        let mut c = 1; // after 你
        insert_char(&mut s, &mut c, '世');
        assert_eq!(s, "你世好");
        assert_eq!(c, 2);
    }

    #[test]
    fn move_right_respects_char_count() {
        let s = "😀ab";
        let mut c = 0;
        move_right(s, &mut c);
        assert_eq!(c, 1);
        move_right(s, &mut c);
        assert_eq!(c, 2);
        move_right(s, &mut c);
        assert_eq!(c, 3);
        move_right(s, &mut c);
        assert_eq!(c, 3); // clamped
    }

    #[test]
    fn split_at_char_with_emoji() {
        let s = "a😀b";
        let (before, after) = split_at_char(s, 1);
        assert_eq!(before, "a");
        assert_eq!(after, "😀b");

        let (before2, after2) = split_at_char(s, 2);
        assert_eq!(before2, "a😀");
        assert_eq!(after2, "b");
    }
}
```

**Step 2: Run tests to verify they pass**

Run: `cargo test ui::input -- --nocapture`
Expected: All 6 tests PASS (this is a new module with tests and implementation together)

**Step 3: Wire helpers into state.rs**

Replace every `input.insert(*cursor, c); *cursor += 1;` pattern with `crate::ui::input::insert_char(input, cursor, c);` and similarly for backspace, move_left, move_right. There are 4 input-handling blocks in state.rs:

- Lines 122-145 (Normal input_focused)
- Lines 216-238 (AddingFriend)
- Lines 394-404 (ViewingChannel is_own)
- Lines 436-444 (SubscribingToChannel)

Also fix `conversation.rs:200-212` (`render_input`): replace `&input[..cursor]` / `&input[cursor..]` with `split_at_char(input, cursor)`.

**Step 4: Update existing state tests to still pass**

Run: `cargo test ui::state -- --nocapture`
Expected: All existing tests PASS

**Step 5: Add integration test for emoji input in state**

Add to `src/ui/state.rs` tests:

```rust
#[test]
fn input_focused_emoji_typing() {
    let mut state = AppState::Normal {
        selected_friend_idx: Some(0),
        conversation_id: None,
        input: String::new(),
        cursor: 0,
        input_focused: true,
        scroll_offset: 0,
    };
    // Type emoji
    state.handle_key(KeyEvent::new(KeyCode::Char('😀'), KeyModifiers::NONE)).unwrap();
    state.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).unwrap();
    match &state {
        AppState::Normal { input, cursor, .. } => {
            assert_eq!(input, "😀a");
            assert_eq!(*cursor, 2); // char count, not byte count
        }
        _ => panic!("Expected Normal state"),
    }
    // Backspace should delete 'a', not corrupt the emoji
    state.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)).unwrap();
    match &state {
        AppState::Normal { input, cursor, .. } => {
            assert_eq!(input, "😀");
            assert_eq!(*cursor, 1);
        }
        _ => panic!("Expected Normal state"),
    }
}
```

**Step 6: Run full test suite**

Run: `cargo test`
Expected: All tests PASS

**Step 7: Commit**

```bash
git add src/ui/input.rs src/ui/mod.rs src/ui/state.rs src/ui/conversation.rs
git commit -m "fix: use char-index cursor to prevent UTF-8 panics in text input"
```

---

### Task 1.2: Fix UTF-8 Byte Slicing in Display Truncation

**Files:**
- Modify: `src/ui/sidebar.rs:87-91` (friend name truncation)
- Modify: `src/ui/modals.rs:79,180` (.onion truncation)
- Modify: `src/ui/app_ui.rs:44` (header onion truncation)
- Modify: `src/ui/channel_feed.rs:28` (publisher onion truncation)
- Modify: `src/db/queries.rs:23` (FriendEntry::display truncation)

**Step 1: Add a safe truncation helper to `src/ui/input.rs`**

```rust
/// Truncate a string to at most `max_chars` characters, appending "…" if truncated.
pub fn truncate_display(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}…", truncated)
    } else {
        s.to_string()
    }
}

/// Truncate a string to at most `max_chars` characters, appending "..." if truncated.
pub fn truncate_display_dots(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
```

Add tests:

```rust
#[test]
fn truncate_ascii() {
    assert_eq!(truncate_display("abcdefghij", 5), "abcde…");
    assert_eq!(truncate_display("abcde", 5), "abcde");
    assert_eq!(truncate_display("abc", 5), "abc");
}

#[test]
fn truncate_multibyte() {
    assert_eq!(truncate_display("你好世界再见", 4), "你好世界…");
}
```

**Step 2: Replace all unsafe byte slicing**

- `sidebar.rs:88-89`: Replace `if name.len() > max_name_len { format!("{}…", &name[..max_name_len]) }` with `crate::ui::input::truncate_display(&name, max_name_len)`
- `modals.rs:78-79`: Replace `if from_onion.len() > 16 { format!("{}…", &from_onion[..16]) }` with `crate::ui::input::truncate_display(from_onion, 16)`
- `modals.rs:179-180`: Same pattern for `req.from_onion`
- `app_ui.rs:43-44`: Replace `if a.len() > 16 { &a[..16] }` with char-safe truncation
- `channel_feed.rs:28`: Replace `&publisher_onion[..12]` with char-safe truncation
- `db/queries.rs:23-24`: Replace `&addr[..12]` with char-safe truncation

**Step 3: Run full test suite**

Run: `cargo test`
Expected: All PASS

**Step 4: Commit**

```bash
git add src/ui/input.rs src/ui/sidebar.rs src/ui/modals.rs src/ui/app_ui.rs src/ui/channel_feed.rs src/db/queries.rs
git commit -m "fix: use char-safe truncation to prevent UTF-8 panics"
```

---

### Task 1.3: Bounds-Check Friend List Navigation

**Files:**
- Modify: `src/ui/state.rs:195-199`
- Modify: `src/ui/state.rs:89` (handle_key signature — needs friend_count)

**Step 1: Write failing test**

Add to `src/ui/state.rs` tests:

```rust
#[test]
fn down_arrow_bounded_by_friend_count() {
    let mut state = AppState::Normal {
        selected_friend_idx: Some(2),
        conversation_id: None,
        input: String::new(),
        cursor: 0,
        input_focused: false,
        scroll_offset: 0,
    };
    // With 3 friends (indices 0,1,2), down from index 2 should stay at 2
    let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    state.handle_key_with_context(key, 3).unwrap();
    match &state {
        AppState::Normal { selected_friend_idx, .. } => {
            assert_eq!(*selected_friend_idx, Some(2));
        }
        _ => panic!("Expected Normal state"),
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test ui::state::tests::down_arrow_bounded_by_friend_count -- --nocapture`
Expected: FAIL (method doesn't exist yet)

**Step 3: Implement the fix**

Add a new method `handle_key_with_context` to `AppState` that takes a friend count. The existing `handle_key` can call it with `usize::MAX` for backwards compatibility, or we can update the caller in `main.rs` to pass the count. The cleanest approach: add `friend_count: usize` parameter to `handle_key` and update the single caller in `main.rs:359`.

In `state.rs`, change the Down arrow handler:

```rust
KeyCode::Down => {
    if let Some(idx) = selected_friend_idx {
        if *idx + 1 < friend_count {
            *idx += 1;
        }
    }
    Ok(None)
}
```

**Step 4: Update all callers and existing tests**

- `main.rs:359`: `app_state.handle_key(key, friends.len())?` — but we need `friends` available. The friends list is queried at line 275 and dropped at line 320, but the event handling is after drop. Solution: store `friends.len()` in a local `let friend_count = friends.len();` before `drop(app_lock)`.
- All existing tests: pass a `friend_count` argument (most can use `10` as a safe upper bound).

**Step 5: Run all tests**

Run: `cargo test`
Expected: All PASS

**Step 6: Commit**

```bash
git add src/ui/state.rs src/main.rs
git commit -m "fix: bounds-check friend list Down arrow navigation"
```

---

### Task 1.4: Replace .expect() on PreKey OPK Decode

**Files:**
- Modify: `src/main.rs:1137-1138` (or nearby — the PreKey decode `.expect()` calls)

**Step 1: Find the exact lines**

Search for `expect("Failed to decode PreKey OPK")` and `expect("PreKey OPK has wrong length")` in main.rs.

**Step 2: Replace with ? propagation**

Change from:
```rust
let decoded = base64::engine::general_purpose::STANDARD
    .decode(&stored_value)
    .expect("Failed to decode PreKey OPK");
```

To:
```rust
let decoded = base64::engine::general_purpose::STANDARD
    .decode(&stored_value)
    .map_err(|e| error::ChattorError::Crypto(format!("Failed to decode PreKey material: {}", e)))?;
```

Do the same for any `.expect()` on length checks — convert to a proper error return.

**Step 3: Run tests**

Run: `cargo test`
Expected: All PASS

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "fix: replace .expect() with error propagation for PreKey decode"
```

---

### Task 1.5: Add Flash Message System and Wire Up Error Surfacing

**Files:**
- Modify: `src/ui/app_ui.rs:12-28` (RenderContext — add `status_flash` field)
- Modify: `src/main.rs` (replace `eprintln!` calls with flash messages)
- Modify: `src/ui/error.rs` (remove `#[allow(dead_code)]`)

**Step 1: Extend the existing notification_flash to a general status_flash**

The `notification_flash` field in main.rs already implements the pattern we need: `Option<(Instant, &str)>` with 2-second expiry. We'll generalize it:

In `main.rs`, rename `notification_flash` to `status_flash` and change it to `Option<(std::time::Instant, String)>` (owned String so we can store dynamic error messages).

Update `RenderContext::notification_flash` to `status_flash: Option<String>`.

**Step 2: Replace key eprintln! calls**

Replace the highest-impact invisible errors with flash messages. Focus on these:

- Line 387: `eprintln!("Failed to accept friend request: {}", e)` → `status_flash = Some((Instant::now(), format_error_for_user(&e)));`
- Line 408: `eprintln!("Failed to reject friend request: {}", e)` → same
- Line 579: `eprintln!("Failed to encrypt message: no session for {}", peer_onion)` → `status_flash = Some((Instant::now(), "Message failed — no encryption session".to_string()));`

Add success flashes:
- After friend request accept: `status_flash = Some((Instant::now(), "Friend request accepted".to_string()));`
- After friend request reject: `status_flash = Some((Instant::now(), "Friend request rejected".to_string()));`

**Step 3: Remove `#[allow(dead_code)]` from format_error_for_user**

In `src/ui/error.rs:3`, remove the `#[allow(dead_code)]` attribute now that it's used.

**Step 4: Guard identity modal copy when Tor not ready**

In `src/ui/state.rs:340-351` (ViewingMyIdentity key handlers), check if `onion_address` starts with `"("` before allowing copy:

```rust
KeyCode::Char('o') | KeyCode::Char('1') => {
    if !onion_address.starts_with('(') {
        if crate::ui::copy_to_clipboard(onion_address) {
            *copied_field = Some("onion".into());
        }
    }
    Ok(None)
}
```

**Step 5: Run tests**

Run: `cargo test`
Expected: All PASS

**Step 6: Commit**

```bash
git add src/main.rs src/ui/app_ui.rs src/ui/error.rs src/ui/state.rs
git commit -m "feat: surface errors as in-TUI flash messages, replace invisible eprintln"
```

---

## Tract 2: Render Loop & Mutex Architecture

### Task 2.1: Extract ConnectionPool from App Mutex

This is the most impactful change. Background tasks currently lock the entire `App` just to access `connection_pool`. We'll make the pool a top-level `Arc<ConnectionPool>` that's passed independently.

**Files:**
- Modify: `src/app.rs:17-28` (App struct)
- Modify: `src/main.rs:222-258` (heartbeat task)
- Modify: `src/main.rs:770-785` (queue processor)
- Modify: `src/main.rs` (init_tor result handling — extract pool from App)

**Step 1: Move ConnectionPool to a top-level Arc**

In `main.rs`, after Tor init completes, extract the pool:

```rust
// After bootstrap phase, extract pool for independent use
let connection_pool: Arc<Mutex<Option<Arc<ConnectionPool>>>> = Arc::new(Mutex::new(None));
```

When Tor init succeeds (line ~190 area), set it:

```rust
let pool = {
    let app_lock = app.lock().await;
    app_lock.connection_pool.clone()
};
*connection_pool.lock().await = pool;
```

Better approach: use a `tokio::sync::watch` channel to broadcast the pool once it's available:

```rust
let (pool_tx, pool_rx) = tokio::sync::watch::channel::<Option<Arc<ConnectionPool>>>(None);
```

Heartbeat and queue tasks receive `pool_rx.clone()` and wait for Some(pool).

**Step 2: Restructure heartbeat to not hold app lock during sends**

```rust
let pool_rx_heartbeat = pool_rx.clone();
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(15)).await;
    loop {
        // Wait for pool to be available
        let pool = {
            let rx = pool_rx_heartbeat.borrow();
            rx.clone()
        };

        if let Some(pool) = pool {
            let peers = pool.connected_peers();
            let now = SystemTime::now()...;

            // Build all messages without any lock
            let mut tasks = tokio::task::JoinSet::new();
            for peer in peers {
                let msg = Message::Presence(...);
                let pool = Arc::clone(&pool);
                tasks.spawn(async move {
                    let _ = pool.send(&peer, &msg).await;
                });
            }
            // Wait for all sends (concurrent, no app lock held)
            while let Some(_) = tasks.join_next().await {}
        }

        tokio::time::sleep(presence::HEARTBEAT_INTERVAL).await;
    }
});
```

The heartbeat only needs `own_onion` (which doesn't change after Tor init) and the pool. It no longer needs `app` at all.

**Step 3: Restructure queue processor similarly**

The queue processor needs `app.db` and `app.message_queue` to read the queue, plus `pool` to send. We can lock app briefly to read the queue, then release and send without lock.

**Step 4: Run tests**

Run: `cargo test`
Expected: All PASS

**Step 5: Commit**

```bash
git add src/main.rs src/app.rs
git commit -m "perf: extract ConnectionPool from App mutex for lock-free background sends"
```

---

### Task 2.2: Add Dirty Flag to Render Loop

**Files:**
- Modify: `src/main.rs:270-356` (render loop)

**Step 1: Add dirty flag and throttled cleanup**

```rust
let mut dirty = true; // Start dirty to force initial render
let mut last_cleanup = std::time::Instant::now();
let cleanup_interval = std::time::Duration::from_secs(30);
```

At the top of the loop, only re-query if dirty:

```rust
if dirty {
    let app_lock = app.lock().await;
    // ... all the DB queries ...
    drop(app_lock);
    dirty = false;
}
```

Set `dirty = true` whenever:
- A key event is received
- An incoming message arrives (from `incoming_message_rx`)
- A timer fires (e.g., every 5 seconds for presence updates)

Replace unconditional `cleanup_expired_messages` with:
```rust
if last_cleanup.elapsed() > cleanup_interval {
    db::queries::cleanup_expired_messages(&app_lock.db).ok();
    last_cleanup = std::time::Instant::now();
}
```

**Step 2: Add periodic refresh timer for presence/typing**

The typing indicator needs to update when it expires (5s). Add a 1-second "presence tick" that sets dirty:

```rust
let mut presence_tick = tokio::time::interval(Duration::from_secs(1));
```

In the event loop, use `tokio::select!` instead of `event::poll`:

```rust
tokio::select! {
    _ = presence_tick.tick() => { dirty = true; }
    result = async { event::poll(Duration::from_millis(50)).map(|ready| if ready { Some(event::read()) } else { None }) } => { ... }
    msg = incoming_rx.recv() => { handle_incoming(msg); dirty = true; }
}
```

Note: crossterm event polling doesn't play perfectly with tokio::select. The pragmatic approach is to keep the polling loop but only re-query DB when dirty, and set dirty on a 1-second timer for presence.

**Step 3: Run tests**

Run: `cargo test`
Expected: All PASS (no test touches the main event loop directly)

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "perf: add dirty flag to render loop — skip DB queries when idle"
```

---

### Task 2.3: Batch Channel Post Read Counts

**Files:**
- Modify: `src/db/queries.rs` (add batch query)
- Modify: `src/main.rs:307-313` (use batch query)

**Step 1: Write failing test for batch query**

Add to `src/db/queries.rs` tests:

```rust
#[test]
fn test_batch_channel_post_read_counts() {
    let temp = NamedTempFile::new().unwrap();
    let db = Database::open(temp.path()).unwrap();

    store_channel_post_receipt(&db, "post-1", "bob.onion", 1000).unwrap();
    store_channel_post_receipt(&db, "post-1", "carol.onion", 2000).unwrap();
    store_channel_post_receipt(&db, "post-2", "bob.onion", 3000).unwrap();

    let counts = get_channel_post_read_counts_batch(&db, &["post-1", "post-2", "post-3"]).unwrap();
    assert_eq!(counts.get("post-1"), Some(&2));
    assert_eq!(counts.get("post-2"), Some(&1));
    assert_eq!(counts.get("post-3"), None); // no receipts
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test db::queries::tests::test_batch_channel_post_read_counts -- --nocapture`
Expected: FAIL (function doesn't exist)

**Step 3: Implement batch query**

Add to `src/db/queries.rs`:

```rust
/// Get read counts for multiple posts in a single query (publisher side).
/// Returns a map of post_id -> count. Posts with zero reads are omitted.
pub fn get_channel_post_read_counts_batch(
    db: &Database,
    post_ids: &[&str],
) -> Result<std::collections::HashMap<String, i64>> {
    if post_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let placeholders: Vec<String> = (1..=post_ids.len()).map(|i| format!("?{}", i)).collect();
    let sql = format!(
        "SELECT post_id, COUNT(*) FROM channel_post_receipts WHERE post_id IN ({}) GROUP BY post_id",
        placeholders.join(", ")
    );

    let conn = db.connection();
    let mut stmt = conn.prepare(&sql)
        .map_err(|e| ChattorError::Database(format!("Failed to prepare batch read counts: {}", e)))?;

    let params: Vec<&dyn rusqlite::types::ToSql> = post_ids.iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    }).map_err(|e| ChattorError::Database(format!("Failed to query batch read counts: {}", e)))?;

    let mut counts = std::collections::HashMap::new();
    for row in rows {
        let (post_id, count) = row
            .map_err(|e| ChattorError::Database(format!("Failed to read batch count row: {}", e)))?;
        counts.insert(post_id, count);
    }

    Ok(counts)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test db::queries::tests::test_batch_channel_post_read_counts -- --nocapture`
Expected: PASS

**Step 5: Wire into main.rs**

Replace `main.rs:307-313`:

```rust
let mut counts = std::collections::HashMap::new();
if *is_own && !posts.is_empty() {
    let post_ids: Vec<&str> = posts.iter().map(|p| p.post_id.as_str()).collect();
    counts = db::queries::get_channel_post_read_counts_batch(&app_lock.db, &post_ids)
        .unwrap_or_default();
}
```

**Step 6: Run full tests**

Run: `cargo test`
Expected: All PASS

**Step 7: Commit**

```bash
git add src/db/queries.rs src/main.rs
git commit -m "perf: batch channel post read counts into single GROUP BY query"
```

---

### Task 2.4: Enable WAL Mode and SQLite Pragmas

**Files:**
- Modify: `src/db/connection.rs:13-22` (after Connection::open)

**Step 1: Add pragmas after opening connection**

In `Database::open`, right after creating the connection:

```rust
// Optimize SQLite for concurrent read/write workload
conn.execute_batch(
    "PRAGMA journal_mode=WAL;
     PRAGMA synchronous=NORMAL;
     PRAGMA cache_size=-8000;
     PRAGMA busy_timeout=5000;"
).map_err(|e| ChattorError::Database(format!("Failed to set pragmas: {}", e)))?;
```

**Step 2: Run tests**

Run: `cargo test`
Expected: All PASS

**Step 3: Commit**

```bash
git add src/db/connection.rs
git commit -m "perf: enable WAL mode and SQLite pragmas for better concurrency"
```

---

### Task 2.5: Fix ConnectionPool DashMap Lock Duration

**Files:**
- Modify: `src/net/pool.rs:68-87`

**Step 1: Restructure send to release DashMap lock before I/O**

The current code holds `get_mut()` across the entire `pool.send().await`. Instead, take the connection out of the map, send, then put it back:

```rust
pub async fn send(&self, peer_onion: &str, message: &Message) -> Result<()> {
    // Try to take a cached connection (releases DashMap lock immediately)
    let cached = self.connections.remove(peer_onion).map(|(_, pc)| pc);

    if let Some(mut pooled) = cached {
        pooled.last_used = Instant::now();
        let send_result = tokio::time::timeout(
            SEND_TIMEOUT,
            pooled.conn.send(message),
        ).await;

        match send_result {
            Ok(Ok(())) => {
                // Put connection back in pool
                self.connections.insert(peer_onion.to_string(), pooled);
                return Ok(());
            }
            _ => {
                // Stale connection — don't put it back, fall through to fresh
                tracing::debug!("Evicted stale connection to {}", peer_onion);
            }
        }
    }

    // Create fresh connection (no DashMap lock held)
    let mut conn = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TorConnection::connect(&self.tor_client, peer_onion),
    ).await
    .map_err(|_| ChattorError::ConnectionTimeout(peer_onion.to_string()))??;

    tokio::time::timeout(SEND_TIMEOUT, conn.send(message))
        .await
        .map_err(|_| ChattorError::Network(
            format!("Send timed out ({}s) to {}", SEND_TIMEOUT.as_secs(), peer_onion),
        ))??;

    if self.connections.len() >= MAX_POOL_SIZE {
        self.evict_oldest_idle();
    }

    self.connections.insert(peer_onion.to_string(), PooledConnection {
        conn,
        last_used: Instant::now(),
    });

    Ok(())
}
```

**Step 2: Run tests**

Run: `cargo test net::pool -- --nocapture`
Expected: All PASS

**Step 3: Commit**

```bash
git add src/net/pool.rs
git commit -m "perf: release DashMap lock before I/O in ConnectionPool::send"
```

---

## Tract 3: Input & Navigation Polish

### Task 3.1: Add Standard Text Editing Keybindings

**Files:**
- Modify: `src/ui/input.rs` (add helper functions)
- Modify: `src/ui/state.rs` (add keybindings to all input states)

**Step 1: Add helpers to input.rs**

```rust
/// Move cursor to start of input.
pub fn move_to_start(cursor: &mut usize) {
    *cursor = 0;
}

/// Move cursor to end of input.
pub fn move_to_end(input: &str, cursor: &mut usize) {
    *cursor = input.chars().count();
}

/// Delete character under the cursor (forward delete).
pub fn delete_forward(input: &mut String, cursor: &usize) {
    let char_count = input.chars().count();
    if *cursor < char_count {
        let byte_pos = char_to_byte(input, *cursor);
        input.remove(byte_pos);
    }
}

/// Delete from cursor to start of line (Ctrl+U).
pub fn delete_to_start(input: &mut String, cursor: &mut usize) {
    if *cursor > 0 {
        let byte_pos = char_to_byte(input, *cursor);
        input.drain(..byte_pos);
        *cursor = 0;
    }
}

/// Delete word backward (Ctrl+W).
pub fn delete_word_backward(input: &mut String, cursor: &mut usize) {
    if *cursor == 0 { return; }

    let chars: Vec<char> = input.chars().collect();
    let mut new_cursor = *cursor;

    // Skip trailing spaces
    while new_cursor > 0 && chars[new_cursor - 1] == ' ' {
        new_cursor -= 1;
    }
    // Skip word characters
    while new_cursor > 0 && chars[new_cursor - 1] != ' ' {
        new_cursor -= 1;
    }

    let start_byte = char_to_byte(input, new_cursor);
    let end_byte = char_to_byte(input, *cursor);
    input.drain(start_byte..end_byte);
    *cursor = new_cursor;
}
```

Add tests for each helper.

**Step 2: Wire into state.rs input handlers**

In the `input_focused` block of Normal state:

```rust
KeyCode::Home => { crate::ui::input::move_to_start(cursor); Ok(None) }
KeyCode::End => { crate::ui::input::move_to_end(input, cursor); Ok(None) }
KeyCode::Delete => { crate::ui::input::delete_forward(input, cursor); Ok(None) }
KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    crate::ui::input::move_to_start(cursor); Ok(None)
}
KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    crate::ui::input::move_to_end(input, cursor); Ok(None)
}
KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    crate::ui::input::delete_word_backward(input, cursor); Ok(None)
}
KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    crate::ui::input::delete_to_start(input, cursor); Ok(None)
}
```

Repeat for AddingFriend, ViewingChannel, SubscribingToChannel input blocks.

**Step 3: Run tests**

Run: `cargo test`
Expected: All PASS

**Step 4: Commit**

```bash
git add src/ui/input.rs src/ui/state.rs
git commit -m "feat: add Home/End/Delete/Ctrl+A/E/W/U keybindings to text input"
```

---

### Task 3.2: Add Message Scrolling (PageUp/PageDown)

**Files:**
- Modify: `src/ui/state.rs` (Normal navigation mode — add PageUp/PageDown)

**Step 1: Write test**

```rust
#[test]
fn page_up_increases_scroll_offset() {
    let mut state = AppState::Normal {
        selected_friend_idx: Some(0),
        conversation_id: Some(1),
        input: String::new(),
        cursor: 0,
        input_focused: false,
        scroll_offset: 0,
    };
    state.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), 1).unwrap();
    match &state {
        AppState::Normal { scroll_offset, .. } => {
            assert_eq!(*scroll_offset, 10);
        }
        _ => panic!("Expected Normal state"),
    }
}
```

**Step 2: Implement**

In the Normal navigation mode `match key.code` block:

```rust
KeyCode::PageUp => {
    if let AppState::Normal { scroll_offset, .. } = self {
        *scroll_offset = scroll_offset.saturating_add(10);
    }
    Ok(None)
}
KeyCode::PageDown => {
    if let AppState::Normal { scroll_offset, .. } = self {
        *scroll_offset = scroll_offset.saturating_sub(10);
    }
    Ok(None)
}
```

Also handle these in `input_focused` mode (so users can scroll while composing).

**Step 3: Run tests**

Run: `cargo test`
Expected: All PASS

**Step 4: Commit**

```bash
git add src/ui/state.rs
git commit -m "feat: add PageUp/PageDown for message scrolling"
```

---

### Task 3.3: Add Vim-Style j/k Navigation

**Files:**
- Modify: `src/ui/state.rs` (Normal navigation mode)

**Step 1: Add j/k alongside Up/Down**

In the Normal navigation mode block, add aliases:

```rust
KeyCode::Char('j') => {
    // Same as Down
    if let Some(idx) = selected_friend_idx {
        if *idx + 1 < friend_count {
            *idx += 1;
        }
    }
    Ok(None)
}
KeyCode::Char('k') => {
    // Same as Up
    if let Some(idx) = selected_friend_idx {
        if *idx > 0 {
            *idx -= 1;
        }
    }
    Ok(None)
}
```

Also add j/k to `ViewingFriendRequests` and `SettingEphemeral` navigation.

**Step 2: Write test and run**

```rust
#[test]
fn vim_jk_navigation() {
    let mut state = AppState::Normal {
        selected_friend_idx: Some(0),
        ..Default::default() // won't work with enum, spell it out
    };
    // ... test j increments, k decrements
}
```

**Step 3: Commit**

```bash
git add src/ui/state.rs
git commit -m "feat: add vim-style j/k navigation alongside arrow keys"
```

---

### Task 3.4: Flash Message for Empty Friend Requests

**Files:**
- Modify: `src/main.rs` (ViewFriendRequests handler, ~line 423-433)

**Step 1: Add flash when no requests**

```rust
Some(AppAction::ViewFriendRequests) => {
    let app_lock = app.lock().await;
    let requests = db::queries::get_pending_friend_requests(&app_lock.db).unwrap_or_default();
    drop(app_lock);
    if !requests.is_empty() {
        app_state = AppState::ViewingFriendRequests {
            requests,
            selected_idx: 0,
        };
    } else {
        status_flash = Some((std::time::Instant::now(), "No pending friend requests".to_string()));
    }
}
```

**Step 2: Run tests, commit**

```bash
git add src/main.rs
git commit -m "feat: show flash message when no pending friend requests"
```

---

## Tract 4: Visual & Feedback Polish

### Task 4.1: Fix "Connecting..." After Continue Offline

**Files:**
- Modify: `src/ui/app_ui.rs:49-53`
- Modify: `src/main.rs` (add `continued_offline` flag)

**Step 1: Add `continued_offline` to RenderContext**

Add field `pub continued_offline: bool` to `RenderContext`.

**Step 2: Set it when user chooses Continue Offline**

In the bootstrap failure handler where `BootstrapAction::ContinueOffline` is handled, set a flag.

**Step 3: Update header rendering**

```rust
let (tor_icon, tor_label, tor_color) = if ctx.tor_connected {
    ("\u{25c9}", "Connected", ctx.theme.success)
} else if ctx.continued_offline {
    ("\u{25cb}", "Offline", ctx.theme.fg_dim)
} else {
    ("\u{25cc}", "Connecting...", ctx.theme.warning)
};
```

**Step 4: Commit**

```bash
git add src/ui/app_ui.rs src/main.rs
git commit -m "fix: show 'Offline' instead of 'Connecting...' after Continue Offline"
```

---

### Task 4.2: Fix Cyberpunk Theme Contrast

**Files:**
- Modify: `src/ui/theme.rs:161-194` (cyberpunk preset)

**Step 1: Identify and fix low-contrast colors**

Change these in the cyberpunk theme:
- `fg_dim`: `#005500` → `#00AA00` (brighter green, ~4.5:1 contrast on black)
- `border`: `#003300` → `#006600` (visible borders)
- `msg_timestamp`: `#005500` → `#00AA00`
- `msg_status_sent`: `#005500` → `#00AA00`

**Step 2: Run tests, commit**

```bash
git add src/ui/theme.rs
git commit -m "fix: improve cyberpunk theme contrast ratios for accessibility"
```

---

### Task 4.3: Show Current Setting in Ephemeral Modal

**Files:**
- Modify: `src/ui/state.rs` (SettingEphemeral initialization)
- Modify: `src/main.rs` (pass current TTL when entering ephemeral modal)

**Step 1: Initialize selected_idx based on current TTL**

When entering `SettingEphemeral`, compute the initial `selected_idx` from the conversation's current TTL:

```rust
KeyCode::Char('e') => {
    if let Some(conv_id) = *conversation_id {
        let current_ttl = current_ephemeral_ttl; // from render context or stored
        let selected_idx = match current_ttl {
            None => 0,
            Some(300) => 1,
            Some(3600) => 2,
            Some(86400) => 3,
            Some(604800) => 4,
            Some(_) => 0,
        };
        *self = AppState::SettingEphemeral {
            conversation_id: conv_id,
            selected_idx,
        };
    }
    Ok(None)
}
```

This requires passing `conversation_ephemeral_ttl` to the state handler. Since `handle_key` currently doesn't have this data, we can handle it in main.rs by checking the TTL after the action is returned, or by storing the current TTL in the Normal state.

Simplest approach: handle it in `main.rs` — when we get the SetEphemeralTtl action back, the state has already transitioned. Instead, intercept the state transition in main.rs: after `handle_key`, if state became `SettingEphemeral`, look up the current TTL and adjust `selected_idx`.

**Step 2: Commit**

```bash
git add src/main.rs src/ui/state.rs
git commit -m "feat: ephemeral modal highlights current TTL setting"
```

---

### Task 4.4: Reserve Space for Typing Indicator

**Files:**
- Modify: `src/ui/conversation.rs:100-117`

**Step 1: Adjust the message area when typing**

When `friend_is_typing` is true, subtract 1 from the message area height to reserve space for the typing indicator, rather than overlaying it on the last message.

```rust
let (msg_area, typing_area) = if friend_is_typing {
    let msg = Rect { ..padded, height: padded.height.saturating_sub(1) };
    let typing = Rect { y: padded.y + msg.height, height: 1, ..padded };
    (msg, Some(typing))
} else {
    (padded, None)
};
```

**Step 2: Commit**

```bash
git add src/ui/conversation.rs
git commit -m "fix: reserve dedicated space for typing indicator"
```

---

### Task 4.5: Add Date Separators Between Messages

**Files:**
- Modify: `src/ui/conversation.rs:131-171` (render_messages)

**Step 1: Insert date separator lines**

In the `render_messages` loop, track the last message's date. When a new message is on a different day, insert a separator line:

```rust
let mut last_date: Option<String> = None;

for msg in messages {
    let msg_date = format_date(msg.timestamp); // "2026-02-25"
    if last_date.as_ref() != Some(&msg_date) {
        if last_date.is_some() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!("─── {} ───", msg_date),
            Style::default().fg(theme.fg_dim),
        )));
        lines.push(Line::from(""));
        last_date = Some(msg_date);
    }
    // ... existing message rendering
}
```

Add `format_date` helper:

```rust
fn format_date(ts: i64) -> String {
    // Convert Unix timestamp to YYYY-MM-DD
    let secs = ts;
    let days = secs / 86400;
    // Simple date calculation (or use chrono if already a dep — check Cargo.toml)
    // For now, use the stdlib approach
    let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
    // Without chrono, we can format relatively:
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let today_start = (now_secs / 86400) * 86400;
    let msg_day_start = (ts / 86400) * 86400;

    if msg_day_start == today_start {
        "Today".to_string()
    } else if msg_day_start == today_start - 86400 {
        "Yesterday".to_string()
    } else {
        let days_ago = (today_start - msg_day_start) / 86400;
        format!("{} days ago", days_ago)
    }
}
```

**Step 2: Update `format_timestamp` for older messages**

Messages older than 24h: show absolute time instead of "Xd ago".

**Step 3: Commit**

```bash
git add src/ui/conversation.rs
git commit -m "feat: add date separators and improved timestamps in conversation view"
```

---

## Summary

| Tract | Tasks | Est. Changes |
|-------|-------|-------------|
| 1: Crash Prevention | 5 tasks | ~300 lines |
| 2: Render Loop & Mutex | 5 tasks | ~400 lines |
| 3: Input & Navigation | 4 tasks | ~250 lines |
| 4: Visual Polish | 5 tasks | ~200 lines |
| **Total** | **19 tasks** | **~1150 lines** |

Each task is independently testable and commitable. Run `cargo test` after every task. Run `cargo clippy -- -D warnings` before the final commit of each tract.
