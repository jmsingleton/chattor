use crate::db::Database;

/// Check if notifications are enabled (defaults to true)
pub fn is_enabled(db: &Database) -> bool {
    crate::db::queries::get_app_setting(db, "notifications_enabled")
        .unwrap_or(None)
        .map(|v| v != "false")
        .unwrap_or(true)
}

/// Toggle notifications on/off, returns new state
pub fn toggle(db: &Database) -> bool {
    let current = is_enabled(db);
    let new_state = !current;
    crate::db::queries::set_app_setting(
        db,
        "notifications_enabled",
        if new_state { "true" } else { "false" },
    ).ok();
    new_state
}

/// Send a desktop notification for an incoming message.
/// Does not include message content (privacy-first).
pub fn notify_message(sender_name: &str) {
    if let Err(e) = notify_rust::Notification::new()
        .summary("Chattor")
        .body(&format!("New message from {}", sender_name))
        .icon("mail-unread")
        .timeout(notify_rust::Timeout::Milliseconds(5000))
        .show()
    {
        tracing::warn!(error = %e, "desktop notification failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::NamedTempFile;

    fn test_db() -> (Database, NamedTempFile) {
        let f = NamedTempFile::new().unwrap();
        let db = Database::open(f.path()).unwrap();
        (db, f)
    }

    #[test]
    fn test_notifications_enabled_by_default() {
        let (db, _tmp) = test_db();
        assert!(is_enabled(&db));
    }

    #[test]
    fn test_toggle_notifications() {
        let (db, _tmp) = test_db();
        assert!(is_enabled(&db));
        let new_state = toggle(&db);
        assert!(!new_state);
        assert!(!is_enabled(&db));
        let new_state = toggle(&db);
        assert!(new_state);
        assert!(is_enabled(&db));
    }
}
