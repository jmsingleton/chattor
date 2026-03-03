use crate::app::App;
use crate::crypto;
use crate::db;
use crate::error;
use crate::error::Result;
use crate::net;
use crate::notifications;
use crate::presence;
use crate::protocol;
use base64::Engine;
use std::sync::Arc;

use super::friend_request::handle_incoming_accept;

/// Try to send a message directly to peer via the connection pool
pub async fn try_send_direct(
    app: &App,
    peer_onion: &str,
    message: &protocol::message::Message,
) -> Result<()> {
    let pool = app
        .connection_pool
        .as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Connection pool not initialized".into()))?;

    pool.send(peer_onion, message).await
}

/// Handle an incoming message from the listener
pub async fn handle_incoming_message(
    app: &App,
    incoming: net::listener::IncomingMessage,
    presence: &presence::PresenceMap,
) -> Result<()> {
    // Extract sender onion for rate limiting
    let sender_onion = match &incoming.message {
        protocol::message::Message::FriendRequest(req) => Some(req.from_onion.as_str()),
        protocol::message::Message::FriendRequestAccept(accept) => Some(accept.from_onion.as_str()),
        protocol::message::Message::TextMessage(msg) => Some(msg.from_onion.as_str()),
        protocol::message::Message::Presence(pres) => Some(pres.from_onion.as_str()),
        protocol::message::Message::ChannelSubscribe(sub) => Some(sub.subscriber_onion.as_str()),
        protocol::message::Message::ChannelUnsubscribe(unsub) => {
            Some(unsub.subscriber_onion.as_str())
        }
        protocol::message::Message::ChannelPost(post) => Some(post.publisher_onion.as_str()),
        protocol::message::Message::ChannelSyncRequest(req) => Some(req.subscriber_onion.as_str()),
        protocol::message::Message::ChannelSyncResponse(resp) => {
            Some(resp.publisher_onion.as_str())
        }
        protocol::message::Message::ChannelPostReceipt(receipt) => {
            Some(receipt.reader_onion.as_str())
        }
        _ => None,
    };

    if let Some(peer) = sender_onion {
        if !app.rate_limiter.check(peer) {
            tracing::warn!("Rate limited message from {}", &peer[..8.min(peer.len())]);
            return Ok(());
        }
    }

    match &incoming.message {
        protocol::message::Message::FriendRequest(req) => {
            // Verify Ed25519 signature before storing
            use crate::protocol::friend_request::FriendRequestHandler;
            match FriendRequestHandler::validate_request(req) {
                Ok(true) => {
                    // Signature valid — store the request
                    let conn = app.db.connection();
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    conn.execute(
                        "INSERT INTO friend_requests (from_onion, friend_code, received_at, status)
                         VALUES (?1, ?2, ?3, 'pending')",
                        (&req.from_onion, &req.from_friendcode, now),
                    )
                    .map_err(|e| {
                        error::ChattorError::Database(format!(
                            "Failed to save friend request: {}",
                            e
                        ))
                    })?;

                    eprintln!("Received verified friend request from {}", req.from_onion);
                }
                Ok(false) => {
                    eprintln!(
                        "Rejected friend request from {} (invalid signature)",
                        req.from_onion
                    );
                }
                Err(e) => {
                    eprintln!(
                        "Error validating friend request from {}: {}",
                        req.from_onion, e
                    );
                }
            }
        }
        protocol::message::Message::FriendRequestAccept(accept) => {
            handle_incoming_accept(app, accept)?;
        }
        protocol::message::Message::FriendRequestReject(reject) => {
            eprintln!("Friend request rejected by {}", reject.from_onion);
        }
        protocol::message::Message::TextMessage(text_msg) => {
            let from_onion = &text_msg.from_onion;
            let msg_id = text_msg.message_id.to_string();
            let is_prekey =
                text_msg.signal_type == protocol::message::SignalMessageType::PrekeyMessage;

            // Decode header and ciphertext from wire format
            let store = crypto::SessionStore::new(&app.db);
            let header = base64::engine::general_purpose::STANDARD
                .decode(&text_msg.signal_header)
                .map_err(|e| {
                    error::ChattorError::Crypto(format!("Failed to decode header: {}", e))
                })?;
            let ciphertext = base64::engine::general_purpose::STANDARD
                .decode(&text_msg.signal_ciphertext)
                .map_err(|e| {
                    error::ChattorError::Crypto(format!("Failed to decode ciphertext: {}", e))
                })?;

            let payload = match store.load_session(from_onion)? {
                Some(mut session) => {
                    let plaintext = session.decrypt(&header, &ciphertext)?;
                    store.store_session(&session)?;
                    serde_json::from_slice::<protocol::message::PlaintextPayload>(&plaintext)
                        .map_err(|e| {
                            error::ChattorError::Crypto(format!("Failed to parse payload: {}", e))
                        })?
                }
                None if is_prekey => {
                    // No session yet — create one from stored PreKey private material.
                    // This happens when we accepted a friend request (stored our private
                    // keys) and the peer sends their first message as a PreKey message.

                    // Extract X3DH init data from the message
                    let x3dh_init = text_msg.x3dh_init.as_ref().ok_or_else(|| {
                        error::ChattorError::Crypto(format!(
                            "PreKey message from {} missing X3DH init data",
                            from_onion
                        ))
                    })?;

                    let alice_identity_bytes = base64::engine::general_purpose::STANDARD
                        .decode(&x3dh_init.sender_identity_key)
                        .map_err(|e| {
                            error::ChattorError::Crypto(format!(
                                "Failed to decode sender identity key: {}",
                                e
                            ))
                        })?;
                    let alice_identity_public: [u8; 33] =
                        alice_identity_bytes.try_into().map_err(|_| {
                            error::ChattorError::Crypto(
                                "Sender identity key has wrong length (expected 33)".into(),
                            )
                        })?;

                    let alice_ephemeral_bytes = base64::engine::general_purpose::STANDARD
                        .decode(&x3dh_init.sender_ephemeral_key)
                        .map_err(|e| {
                            error::ChattorError::Crypto(format!(
                                "Failed to decode sender ephemeral key: {}",
                                e
                            ))
                        })?;
                    let alice_ephemeral_public: [u8; 33] =
                        alice_ephemeral_bytes.try_into().map_err(|_| {
                            error::ChattorError::Crypto(
                                "Sender ephemeral key has wrong length (expected 33)".into(),
                            )
                        })?;

                    // Load all stored PreKey private material
                    let conn = app.db.connection();
                    let identity_b64: String = conn
                        .query_row(
                            "SELECT value FROM app_settings WHERE key = ?1",
                            [&format!("prekey_identity:{}", from_onion)],
                            |row| row.get(0),
                        )
                        .map_err(|_| {
                            error::ChattorError::Crypto(format!(
                                "No stored PreKey identity material for {}",
                                from_onion
                            ))
                        })?;
                    let spk_b64: String = conn
                        .query_row(
                            "SELECT value FROM app_settings WHERE key = ?1",
                            [&format!("prekey_spk:{}", from_onion)],
                            |row| row.get(0),
                        )
                        .map_err(|_| {
                            error::ChattorError::Crypto(format!(
                                "No stored PreKey SPK material for {}",
                                from_onion
                            ))
                        })?;
                    let opk_b64: Option<String> = conn
                        .query_row(
                            "SELECT value FROM app_settings WHERE key = ?1",
                            [&format!("prekey_opk:{}", from_onion)],
                            |row| row.get(0),
                        )
                        .ok();

                    let identity_secret: [u8; 32] = base64::engine::general_purpose::STANDARD
                        .decode(&identity_b64)
                        .map_err(|e| {
                            error::ChattorError::Crypto(format!(
                                "Failed to decode PreKey identity: {}",
                                e
                            ))
                        })?
                        .try_into()
                        .map_err(|_| {
                            error::ChattorError::Crypto(
                                "PreKey identity secret has wrong length".into(),
                            )
                        })?;
                    let signed_prekey_secret: [u8; 32] = base64::engine::general_purpose::STANDARD
                        .decode(&spk_b64)
                        .map_err(|e| {
                            error::ChattorError::Crypto(format!(
                                "Failed to decode PreKey SPK: {}",
                                e
                            ))
                        })?
                        .try_into()
                        .map_err(|_| {
                            error::ChattorError::Crypto("PreKey SPK secret has wrong length".into())
                        })?;
                    let prekey_secret: Option<[u8; 32]> = opk_b64
                        .map(|b64| {
                            let bytes = base64::engine::general_purpose::STANDARD
                                .decode(&b64)
                                .map_err(|e| {
                                    error::ChattorError::Crypto(format!(
                                        "Failed to decode PreKey OPK: {}",
                                        e
                                    ))
                                })?;
                            bytes.try_into().map_err(|_| {
                                error::ChattorError::Crypto("PreKey OPK has wrong length".into())
                            })
                        })
                        .transpose()?;

                    let private_material = crypto::PreKeyPrivateMaterial {
                        identity_secret,
                        signed_prekey_secret,
                        prekey_secret,
                    };

                    let (mut session, _ad) = crypto::SignalSession::from_prekey_message_real(
                        from_onion.clone(),
                        &private_material,
                        &alice_identity_public,
                        &alice_ephemeral_public,
                    )?;

                    let plaintext = session.decrypt(&header, &ciphertext)?;
                    store.store_session(&session)?;

                    // Clean up stored PreKey material (session is now established)
                    conn.execute(
                        "DELETE FROM app_settings WHERE key LIKE ?1",
                        [&format!("prekey_%:{}", from_onion)],
                    )
                    .ok();
                    conn.execute(
                        "DELETE FROM app_settings WHERE key = ?1",
                        [&format!("signal_identity_secret:{}", from_onion)],
                    )
                    .ok();

                    serde_json::from_slice::<protocol::message::PlaintextPayload>(&plaintext)
                        .map_err(|e| {
                            error::ChattorError::Crypto(format!("Failed to parse payload: {}", e))
                        })?
                }
                None => {
                    eprintln!(
                        "No session for {} and not a PreKey message, cannot decrypt",
                        from_onion
                    );
                    return Ok(());
                }
            };

            // Handshake messages are session-establishment only — don't display
            if payload.message_type == "handshake" {
                eprintln!("Session established with {} via handshake", from_onion);
                return Ok(());
            }

            // Find friend and conversation
            if let Some(friend_id) = db::queries::find_friend_by_onion(&app.db, from_onion)? {
                let conv_id = db::queries::get_or_create_conversation(&app.db, friend_id)?;
                db::queries::store_incoming_message_with_ttl(
                    &app.db,
                    conv_id,
                    from_onion,
                    &payload.content,
                    &msg_id,
                    payload.ephemeral_ttl,
                )?;

                // Desktop notification (best-effort)
                if notifications::is_enabled(&app.db) {
                    let sender_name = db::queries::get_friend_display_name(&app.db, from_onion)
                        .unwrap_or_else(|_| from_onion.to_string());
                    notifications::notify_message(&sender_name);
                }

                // Queue delivery receipt back to sender
                let receipt = protocol::message::DeliveryReceiptMessage {
                    message_id: text_msg.message_id,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                };
                let receipt_msg = protocol::message::Message::DeliveryReceipt(receipt);
                app.message_queue
                    .enqueue(&app.db, from_onion, &receipt_msg, "high")
                    .ok();
            }
        }
        protocol::message::Message::DeliveryReceipt(receipt) => {
            db::queries::update_message_status(
                &app.db,
                &receipt.message_id.to_string(),
                "delivered",
            )
            .ok();
        }
        protocol::message::Message::ReadReceipt(receipt) => {
            db::queries::update_message_status(&app.db, &receipt.message_id.to_string(), "read")
                .ok();
        }
        protocol::message::Message::ChannelSubscribe(sub) => {
            // Check if subscriber is blocked
            let blocked: bool = app
                .db
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM blocked_onions WHERE onion_address = ?1",
                    [&sub.subscriber_onion],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap_or(false);

            if !blocked {
                let channel_type = match sub.channel_type {
                    protocol::message::ChannelType::Public => "public",
                    protocol::message::ChannelType::FriendsOnly => "friends_only",
                };

                // For friends_only, verify they are a friend
                if channel_type == "friends_only"
                    && db::queries::find_friend_by_onion(&app.db, &sub.subscriber_onion)?.is_none()
                {
                    eprintln!(
                        "Rejected friends_only subscription from non-friend {}",
                        sub.subscriber_onion
                    );
                    return Ok(());
                }

                db::queries::add_channel_subscriber(&app.db, &sub.subscriber_onion, channel_type)?;
                eprintln!(
                    "New {} channel subscriber: {}",
                    channel_type, sub.subscriber_onion
                );
            }
        }
        protocol::message::Message::ChannelUnsubscribe(unsub) => {
            let channel_type = match unsub.channel_type {
                protocol::message::ChannelType::Public => "public",
                protocol::message::ChannelType::FriendsOnly => "friends_only",
            };
            db::queries::remove_channel_subscriber(&app.db, &unsub.subscriber_onion, channel_type)?;
            eprintln!(
                "Unsubscribed: {} from {} channel",
                unsub.subscriber_onion, channel_type
            );
        }
        protocol::message::Message::ChannelPost(post) => {
            // Store remote post (channel_id 0 for remote posts)
            db::queries::store_channel_post(
                &app.db,
                0,
                &post.content,
                &post.post_id.to_string(),
                post.created_at,
                &post.signature,
            )?;

            // Send read receipt back to publisher
            let receipt = protocol::message::ChannelPostReceiptMessage {
                post_id: post.post_id,
                reader_onion: app.onion_address.clone().unwrap_or_default(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            };
            let receipt_msg = protocol::message::Message::ChannelPostReceipt(receipt);
            app.message_queue
                .enqueue(&app.db, &post.publisher_onion, &receipt_msg, "low")
                .ok();
        }
        protocol::message::Message::ChannelSyncRequest(req) => {
            let channel_type_str = match req.channel_type {
                protocol::message::ChannelType::Public => "public",
                protocol::message::ChannelType::FriendsOnly => "friends_only",
            };

            // For friends_only, verify they are a friend
            if channel_type_str == "friends_only"
                && db::queries::find_friend_by_onion(&app.db, &req.subscriber_onion)?.is_none()
            {
                return Ok(());
            }

            let channel_id = if channel_type_str == "public" { 1 } else { 2 };
            let posts =
                db::queries::get_channel_posts_since(&app.db, channel_id, req.since_timestamp)?;

            let post_messages: Vec<protocol::message::ChannelPostMessage> = posts
                .into_iter()
                .map(|p| protocol::message::ChannelPostMessage {
                    publisher_onion: app.onion_address.clone().unwrap_or_default(),
                    channel_type: req.channel_type.clone(),
                    post_id: uuid::Uuid::parse_str(&p.post_id)
                        .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    content: p.content,
                    created_at: p.created_at,
                    signature: p.signature,
                })
                .collect();

            if !post_messages.is_empty() {
                let response = protocol::message::Message::ChannelSyncResponse(
                    protocol::message::ChannelSyncResponseMessage {
                        publisher_onion: app.onion_address.clone().unwrap_or_default(),
                        channel_type: req.channel_type.clone(),
                        posts: post_messages,
                    },
                );
                app.message_queue
                    .enqueue(&app.db, &req.subscriber_onion, &response, "normal")
                    .ok();
            }
        }
        protocol::message::Message::ChannelSyncResponse(resp) => {
            for post in &resp.posts {
                db::queries::store_channel_post(
                    &app.db,
                    0,
                    &post.content,
                    &post.post_id.to_string(),
                    post.created_at,
                    &post.signature,
                )?;
            }
            // Update sync time
            let channel_type_str = match resp.channel_type {
                protocol::message::ChannelType::Public => "public",
                protocol::message::ChannelType::FriendsOnly => "friends_only",
            };
            let max_time = resp.posts.iter().map(|p| p.created_at).max().unwrap_or(0);
            if max_time > 0 {
                db::queries::update_subscription_sync_time(
                    &app.db,
                    &resp.publisher_onion,
                    channel_type_str,
                    max_time,
                )?;
            }
        }
        protocol::message::Message::ChannelPostReceipt(receipt) => {
            db::queries::store_channel_post_receipt(
                &app.db,
                &receipt.post_id.to_string(),
                &receipt.reader_onion,
                receipt.timestamp,
            )?;
        }
        protocol::message::Message::Presence(pres) => match pres.presence_type {
            protocol::message::PresenceType::Heartbeat => {
                presence::record_heartbeat(presence, &pres.from_onion).await;
            }
            protocol::message::PresenceType::TypingStarted => {
                presence::record_typing_started(presence, &pres.from_onion).await;
            }
            protocol::message::PresenceType::TypingStopped => {
                presence::record_typing_stopped(presence, &pres.from_onion).await;
            }
        },
    }

    Ok(())
}

/// Process pending messages in the queue with per-peer concurrency
pub async fn process_message_queue(app: &App) -> Result<()> {
    use std::collections::HashMap;
    use tokio::task::JoinSet;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let pending = app.message_queue.get_pending_messages(&app.db, now)?;

    if pending.is_empty() {
        return Ok(());
    }

    // Group messages by peer
    let mut by_peer: HashMap<String, Vec<net::queue::QueuedMessage>> = HashMap::new();
    for msg in pending {
        by_peer.entry(msg.peer_onion.clone()).or_default().push(msg);
    }

    let pool = app
        .connection_pool
        .as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Connection pool not initialized".into()))?;
    let pool = Arc::clone(pool);

    // Semaphore limits concurrent peer tasks to 10
    let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
    let mut join_set = JoinSet::new();

    for (peer_onion, messages) in by_peer {
        let pool = Arc::clone(&pool);
        let sem = Arc::clone(&semaphore);

        join_set.spawn(async move {
            let _permit = match sem.acquire().await {
                Ok(permit) => permit,
                Err(_) => return Vec::new(), // semaphore closed
            };
            let mut results: Vec<(i64, i64, i64, bool)> = Vec::new(); // (id, created_at, retry_count, success)

            for queued in messages {
                let success = pool.send(&peer_onion, &queued.message).await.is_ok();
                results.push((queued.id, queued.created_at, queued.retry_count, success));

                if !success {
                    break;
                }
            }

            results
        });
    }

    // Collect results and update DB
    while let Some(result) = join_set.join_next().await {
        if let Ok(outcomes) = result {
            for (id, created_at, retry_count, success) in outcomes {
                if success {
                    app.message_queue.mark_delivered(&app.db, id)?;
                } else {
                    match net::queue::compute_next_retry(retry_count, created_at, now) {
                        Some(next) => {
                            app.message_queue.schedule_retry(&app.db, id, next)?;
                        }
                        None => {
                            app.message_queue.mark_failed(&app.db, id)?;
                            eprintln!("Message #{} expired after 24h", id);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
