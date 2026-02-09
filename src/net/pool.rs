use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::net::TcpStream;
use crate::error::Result;

/// Pool of active TCP connections
pub struct ConnectionPool {
    connections: Arc<Mutex<HashMap<String, PooledConnection>>>,
    idle_timeout: Duration,
}

struct PooledConnection {
    stream: TcpStream,
    last_used: Instant,
}

impl ConnectionPool {
    /// Create new connection pool
    pub fn new(idle_timeout: Duration) -> Self {
        ConnectionPool {
            connections: Arc::new(Mutex::new(HashMap::new())),
            idle_timeout,
        }
    }

    /// Get existing connection or create new one
    pub async fn get_or_create<F, Fut>(
        &self,
        onion_address: &str,
        create_fn: F,
    ) -> Result<TcpStream>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<TcpStream>>,
    {
        let mut pool = self.connections.lock().await;

        // Check for existing connection
        if let Some(conn) = pool.get(onion_address) {
            let age = conn.last_used.elapsed();

            if age < self.idle_timeout {
                // Try to use existing connection
                // Note: We can't easily clone TcpStream, so we'll remove and return
                let conn = pool.remove(onion_address).unwrap();
                return Ok(conn.stream);
            } else {
                // Connection expired, remove it
                pool.remove(onion_address);
            }
        }

        // Create new connection
        drop(pool); // Release lock during async operation
        let stream = create_fn().await?;

        Ok(stream)
    }

    /// Store connection back in pool after use
    pub async fn return_connection(&self, onion_address: String, stream: TcpStream) {
        let mut pool = self.connections.lock().await;
        pool.insert(onion_address, PooledConnection {
            stream,
            last_used: Instant::now(),
        });
    }

    /// Clean up expired connections
    pub async fn cleanup(&self) {
        let mut pool = self.connections.lock().await;
        pool.retain(|_, conn| conn.last_used.elapsed() < self.idle_timeout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_connection_reuse() {
        // Create a test server to connect to
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let pool = ConnectionPool::new(Duration::from_secs(300));

        // First connection
        let conn1 = pool.get_or_create("test.onion", || async move {
            TcpStream::connect(addr).await.map_err(|e| crate::error::TorrentChatError::Io(e))
        }).await;

        assert!(conn1.is_ok());

        // Return the connection to the pool
        pool.return_connection("test.onion".to_string(), conn1.unwrap()).await;

        // Second request should reuse
        let mut call_count = 0;
        let conn2 = pool.get_or_create("test.onion", || async move {
            call_count += 1;
            panic!("Should not create new connection");
        }).await;

        // Both should succeed (same connection)
        assert!(conn2.is_ok());
    }
}
