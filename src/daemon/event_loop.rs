use crate::app::App;
use crate::error::Result;
use crate::handlers;
use crate::presence;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Run the daemon event loop. Processes incoming messages and queue commands.
/// Blocks until SIGTERM/SIGINT.
pub async fn run(app: Arc<Mutex<App>>) -> Result<()> {
    let presence_map = presence::new_presence_map();

    // Take ownership of channels from app
    let mut incoming_rx = {
        let mut app_lock = app.lock().await;
        app_lock.incoming_message_rx.take()
    };
    let mut queue_rx = {
        let mut app_lock = app.lock().await;
        app_lock.queue_command_rx.take()
    };

    let mut presence_tick = tokio::time::interval(Duration::from_secs(1));
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    eprintln!("Daemon running. Press Ctrl+C to stop.");

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                eprintln!("Shutting down...");
                break;
            }

            msg = async {
                match incoming_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(incoming) = msg {
                    let app_lock = app.lock().await;
                    if let Err(e) = handlers::messaging::handle_incoming_message(
                        &app_lock, incoming, &presence_map,
                    ).await {
                        eprintln!("Incoming message error: {}", e);
                    }
                }
            }

            cmd = async {
                match queue_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if cmd.is_some() {
                    let app_lock = app.lock().await;
                    if let Err(e) = handlers::messaging::process_message_queue(&app_lock).await {
                        eprintln!("Queue processing error: {}", e);
                    }
                }
            }

            _ = presence_tick.tick() => {
                // Presence cleanup handled by in-memory expiry
            }
        }
    }

    Ok(())
}
