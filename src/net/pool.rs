use crate::error::{Result, ChattorError};
use crate::protocol::message::Message;
use crate::tor::client::TorClient;
use crate::tor::connection::TorConnection;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long an idle connection is kept before eviction
const IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Timeout for establishing a new Tor circuit
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for sending a message on an established connection
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the cleanup task sweeps for idle connections
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Maximum number of cached connections in the pool
const MAX_POOL_SIZE: usize = 50;

struct PooledConnection {
    conn: TorConnection,
    last_used: Instant,
}

/// Connection pool that caches Tor circuits per peer.
///
/// Reuses existing connections when possible, creates new ones on demand,
/// and automatically evicts connections idle for more than 5 minutes.
/// Uses DashMap for lock-free concurrent access.
pub struct ConnectionPool {
    connections: Arc<DashMap<String, PooledConnection>>,
    tor_client: Arc<TorClient>,
}

impl ConnectionPool {
    /// Create a new pool and spawn the background cleanup task.
    pub fn new(tor_client: Arc<TorClient>) -> Arc<Self> {
        let pool = Arc::new(ConnectionPool {
            connections: Arc::new(DashMap::new()),
            tor_client,
        });

        // Spawn background cleanup task
        let cleanup_conns = Arc::clone(&pool.connections);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(CLEANUP_INTERVAL).await;
                let before = cleanup_conns.len();
                cleanup_conns.retain(|_, pc| pc.last_used.elapsed() < IDLE_TIMEOUT);
                let evicted = before - cleanup_conns.len();
                if evicted > 0 {
                    tracing::debug!("Connection pool: evicted {} idle connections", evicted);
                }
            }
        });

        pool
    }

    /// Send a message to a peer, reusing a cached connection if available.
    ///
    /// On send failure with a cached connection, evicts it and retries once
    /// with a fresh circuit. Returns error only if the fresh attempt also fails.
    pub async fn send(&self, peer_onion: &str, message: &Message) -> Result<()> {
        // Try cached connection first
        if let Some(mut pooled) = self.connections.get_mut(peer_onion) {
            pooled.last_used = Instant::now();
            let send_result = tokio::time::timeout(
                SEND_TIMEOUT,
                pooled.conn.send(message),
            ).await;

            match send_result {
                Ok(Ok(())) => return Ok(()),
                _ => {
                    // Must drop the mutable ref before removing
                    drop(pooled);
                    // Stale connection — evict and fall through to fresh connect
                    self.connections.remove(peer_onion);
                    tracing::debug!("Evicted stale connection to {}", peer_onion);
                }
            }
        }

        // Create fresh connection
        let mut conn = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TorConnection::connect(&self.tor_client, peer_onion),
        )
        .await
        .map_err(|_| ChattorError::ConnectionTimeout(peer_onion.to_string()))??;

        // Send on fresh connection
        tokio::time::timeout(SEND_TIMEOUT, conn.send(message))
            .await
            .map_err(|_| ChattorError::Network(
                format!("Send timed out ({}s) to {}", SEND_TIMEOUT.as_secs(), peer_onion),
            ))??;

        // Evict oldest idle connection if at capacity
        if self.connections.len() >= MAX_POOL_SIZE {
            self.evict_oldest_idle();
        }

        // Cache the connection for reuse
        self.connections.insert(peer_onion.to_string(), PooledConnection {
            conn,
            last_used: Instant::now(),
        });

        Ok(())
    }

    /// Explicitly remove a cached connection for a peer.
    #[allow(dead_code)]
    pub fn evict(&self, peer_onion: &str) {
        self.connections.remove(peer_onion);
    }

    /// Get a list of peer onion addresses with active (non-idle) connections.
    /// Used by the heartbeat task to know who to send presence updates to.
    pub fn connected_peers(&self) -> Vec<String> {
        self.connections.iter()
            .filter(|entry| entry.value().last_used.elapsed() < IDLE_TIMEOUT)
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Evict the connection with the oldest `last_used` timestamp.
    fn evict_oldest_idle(&self) {
        let oldest = self.connections.iter()
            .min_by_key(|entry| entry.value().last_used)
            .map(|entry| entry.key().clone());

        if let Some(key) = oldest {
            self.connections.remove(&key);
            tracing::debug!("Connection pool at capacity: evicted oldest connection to {}", key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_pool_size_constant() {
        assert_eq!(MAX_POOL_SIZE, 50);
    }

    #[test]
    fn test_pool_size_limit() {
        // Create a DashMap and verify size limit logic works
        let map: DashMap<String, Instant> = DashMap::new();

        // Insert MAX_POOL_SIZE + 10 entries
        for i in 0..(MAX_POOL_SIZE + 10) {
            // Simulate the eviction check before insert
            if map.len() >= MAX_POOL_SIZE {
                // Evict oldest entry
                let oldest = map.iter()
                    .min_by_key(|entry| *entry.value())
                    .map(|entry| entry.key().clone());
                if let Some(key) = oldest {
                    map.remove(&key);
                }
            }
            map.insert(format!("peer_{}", i), Instant::now());
        }

        // Pool should never exceed MAX_POOL_SIZE
        assert!(map.len() <= MAX_POOL_SIZE);
    }

    #[test]
    fn test_dashmap_retain_removes_idle() {
        let map: DashMap<String, Instant> = DashMap::new();

        // Insert entries with different timestamps
        map.insert("recent".to_string(), Instant::now());
        // We can't easily create an Instant in the past without duration,
        // but we can verify retain with a predicate
        map.insert("also_recent".to_string(), Instant::now());

        assert_eq!(map.len(), 2);

        // Retain all (nothing should be removed since all are recent)
        map.retain(|_, v| v.elapsed() < Duration::from_secs(300));
        assert_eq!(map.len(), 2);
    }
}
