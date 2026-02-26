use crate::app::App;
use crate::handlers;
use crate::net::pool::ConnectionPool;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Spawn all background tasks (channel sync, heartbeat, etc.)
pub async fn spawn_all(
    app: Arc<Mutex<App>>,
    pool_rx: tokio::sync::watch::Receiver<Option<Arc<ConnectionPool>>>,
) {
    spawn_channel_sync(Arc::clone(&app));
    spawn_heartbeat(Arc::clone(&app), pool_rx);
}

fn spawn_channel_sync(app: Arc<Mutex<App>>) {
    tokio::spawn(async move {
        // Initial sync after 10 seconds
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        loop {
            {
                let app_lock = app.lock().await;
                if let Ok(requests) = handlers::channels::collect_sync_requests(&app_lock) {
                    for (peer_onion, sync_msg) in requests {
                        app_lock
                            .message_queue
                            .enqueue(&app_lock.db, &peer_onion, &sync_msg, "low")
                            .ok();
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        }
    });
}

fn spawn_heartbeat(
    app: Arc<Mutex<App>>,
    pool_rx: tokio::sync::watch::Receiver<Option<Arc<ConnectionPool>>>,
) {
    tokio::spawn(async move {
        // Wait for Tor to initialize before starting heartbeats
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        // Capture own_onion once (static after Tor init)
        let own_onion = {
            let app_lock = app.lock().await;
            app_lock.onion_address.clone().unwrap_or_default()
        };

        loop {
            // Get pool from watch channel (no app lock needed)
            let pool = { pool_rx.borrow().clone() };

            if let Some(pool) = pool {
                let peers = pool.connected_peers();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                // Send heartbeats concurrently via JoinSet (no app lock held)
                let mut tasks = tokio::task::JoinSet::new();
                for peer in peers {
                    let msg = crate::protocol::message::Message::Presence(
                        crate::protocol::message::PresenceMessage {
                            from_onion: own_onion.clone(),
                            presence_type: crate::protocol::message::PresenceType::Heartbeat,
                            timestamp: now,
                        },
                    );
                    let pool = Arc::clone(&pool);
                    tasks.spawn(async move {
                        let _ = pool.send(&peer, &msg).await;
                    });
                }
                // Wait for all sends to complete
                while tasks.join_next().await.is_some() {}
            }

            tokio::time::sleep(crate::presence::HEARTBEAT_INTERVAL).await;
        }
    });
}
