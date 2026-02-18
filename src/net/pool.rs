use crate::error::{Result, TorrentChatError};
use crate::protocol::message::Message;
use crate::tor::client::TorClient;
use crate::tor::connection::TorConnection;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// How long an idle connection is kept before eviction
const IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Timeout for establishing a new Tor circuit
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for sending a message on an established connection
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the cleanup task sweeps for idle connections
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

struct PooledConnection {
    conn: TorConnection,
    last_used: Instant,
}

/// Connection pool that caches Tor circuits per peer.
///
/// Reuses existing connections when possible, creates new ones on demand,
/// and automatically evicts connections idle for more than 5 minutes.
pub struct ConnectionPool {
    connections: Arc<Mutex<HashMap<String, PooledConnection>>>,
    tor_client: Arc<TorClient>,
}

impl ConnectionPool {
    /// Create a new pool and spawn the background cleanup task.
    pub fn new(tor_client: Arc<TorClient>) -> Arc<Self> {
        let pool = Arc::new(ConnectionPool {
            connections: Arc::new(Mutex::new(HashMap::new())),
            tor_client,
        });

        // Spawn background cleanup task
        let cleanup_conns = Arc::clone(&pool.connections);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(CLEANUP_INTERVAL).await;
                let mut conns = cleanup_conns.lock().await;
                let before = conns.len();
                conns.retain(|_, pc| pc.last_used.elapsed() < IDLE_TIMEOUT);
                let evicted = before - conns.len();
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
        let mut conns = self.connections.lock().await;
        if let Some(pooled) = conns.get_mut(peer_onion) {
            pooled.last_used = Instant::now();
            let send_result = tokio::time::timeout(
                SEND_TIMEOUT,
                pooled.conn.send(message),
            ).await;

            match send_result {
                Ok(Ok(())) => return Ok(()),
                _ => {
                    // Stale connection — evict and fall through to fresh connect
                    conns.remove(peer_onion);
                    tracing::debug!("Evicted stale connection to {}", peer_onion);
                }
            }
        }
        drop(conns);

        // Create fresh connection
        let mut conn = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TorConnection::connect(&self.tor_client, peer_onion),
        )
        .await
        .map_err(|_| TorrentChatError::ConnectionTimeout(peer_onion.to_string()))??;

        // Send on fresh connection
        tokio::time::timeout(SEND_TIMEOUT, conn.send(message))
            .await
            .map_err(|_| TorrentChatError::Network(
                format!("Send timed out ({}s) to {}", SEND_TIMEOUT.as_secs(), peer_onion),
            ))??;

        // Cache the connection for reuse
        let mut conns = self.connections.lock().await;
        conns.insert(peer_onion.to_string(), PooledConnection {
            conn,
            last_used: Instant::now(),
        });

        Ok(())
    }

    /// Explicitly remove a cached connection for a peer.
    pub async fn evict(&self, peer_onion: &str) {
        let mut conns = self.connections.lock().await;
        conns.remove(peer_onion);
    }

    /// Get a list of peer onion addresses with active (non-idle) connections.
    /// Used by the heartbeat task to know who to send presence updates to.
    pub async fn connected_peers(&self) -> Vec<String> {
        let conns = self.connections.lock().await;
        conns.iter()
            .filter(|(_, pc)| pc.last_used.elapsed() < IDLE_TIMEOUT)
            .map(|(k, _)| k.clone())
            .collect()
    }
}
