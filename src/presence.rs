use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// How often to send heartbeats to peers with active connections
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);

/// Mark peer offline after this duration without a heartbeat
pub const OFFLINE_THRESHOLD: Duration = Duration::from_secs(120);

/// Typing indicator expires after this duration
pub const TYPING_TIMEOUT: Duration = Duration::from_secs(5);

/// Minimum interval between outgoing TypingStarted messages
pub const TYPING_DEBOUNCE: Duration = Duration::from_secs(4);

/// Per-peer presence state (in-memory only, never persisted)
#[derive(Debug, Clone)]
pub struct PeerPresence {
    pub last_seen: Instant,
    pub typing_started: Option<Instant>,
}

impl Default for PeerPresence {
    fn default() -> Self {
        PeerPresence {
            last_seen: Instant::now(),
            typing_started: None,
        }
    }
}

impl PeerPresence {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this peer should be considered online
    pub fn is_online(&self) -> bool {
        self.last_seen.elapsed() < OFFLINE_THRESHOLD
    }

    /// Whether this peer is currently typing
    pub fn is_typing(&self) -> bool {
        self.typing_started
            .map(|t| t.elapsed() < TYPING_TIMEOUT)
            .unwrap_or(false)
    }
}

/// Thread-safe presence tracker shared between main loop and background tasks
pub type PresenceMap = Arc<Mutex<HashMap<String, PeerPresence>>>;

/// Create a new empty presence map
pub fn new_presence_map() -> PresenceMap {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Record a heartbeat from a peer
pub async fn record_heartbeat(map: &PresenceMap, onion: &str) {
    let mut m = map.lock().await;
    let entry = m.entry(onion.to_string()).or_insert_with(PeerPresence::new);
    entry.last_seen = Instant::now();
}

/// Record typing started from a peer
pub async fn record_typing_started(map: &PresenceMap, onion: &str) {
    let mut m = map.lock().await;
    let entry = m.entry(onion.to_string()).or_insert_with(PeerPresence::new);
    entry.last_seen = Instant::now();
    entry.typing_started = Some(Instant::now());
}

/// Record typing stopped from a peer
pub async fn record_typing_stopped(map: &PresenceMap, onion: &str) {
    let mut m = map.lock().await;
    if let Some(entry) = m.get_mut(onion) {
        entry.typing_started = None;
    }
}

/// Get a snapshot of online/typing status for all peers
pub async fn get_presence_snapshot(map: &PresenceMap) -> HashMap<String, (bool, bool)> {
    let m = map.lock().await;
    m.iter()
        .map(|(k, v)| (k.clone(), (v.is_online(), v.is_typing())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_presence_initially_online() {
        let p = PeerPresence::new();
        assert!(p.is_online());
        assert!(!p.is_typing());
    }

    #[test]
    fn test_peer_presence_not_typing_by_default() {
        let p = PeerPresence::new();
        assert!(!p.is_typing());
    }

    #[test]
    fn test_peer_presence_typing_active() {
        let mut p = PeerPresence::new();
        p.typing_started = Some(Instant::now());
        assert!(p.is_typing());
    }

    #[test]
    fn test_peer_presence_typing_expired() {
        let mut p = PeerPresence::new();
        p.typing_started = Some(Instant::now() - Duration::from_secs(6));
        assert!(!p.is_typing());
    }

    #[test]
    fn test_peer_presence_offline_after_threshold() {
        let mut p = PeerPresence::new();
        p.last_seen = Instant::now() - Duration::from_secs(121);
        assert!(!p.is_online());
    }

    #[test]
    fn test_peer_presence_online_within_threshold() {
        let mut p = PeerPresence::new();
        p.last_seen = Instant::now() - Duration::from_secs(60);
        assert!(p.is_online());
    }

    #[tokio::test]
    async fn test_record_heartbeat() {
        let map = new_presence_map();
        record_heartbeat(&map, "test.onion").await;
        let snap = get_presence_snapshot(&map).await;
        assert_eq!(snap.get("test.onion"), Some(&(true, false)));
    }

    #[tokio::test]
    async fn test_record_typing() {
        let map = new_presence_map();
        record_typing_started(&map, "test.onion").await;
        let snap = get_presence_snapshot(&map).await;
        assert_eq!(snap.get("test.onion"), Some(&(true, true)));

        record_typing_stopped(&map, "test.onion").await;
        let snap = get_presence_snapshot(&map).await;
        assert_eq!(snap.get("test.onion"), Some(&(true, false)));
    }
}
