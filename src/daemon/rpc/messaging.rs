use super::RpcResponse;
use crate::app::App;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) async fn handle_send_message(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let peer = match params.get("peer").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'peer' parameter".into()),
    };
    let message = match params.get("message").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'message' parameter".into()),
    };

    // Resolve peer (onion or friend code)
    let peer_onion = if peer.ends_with(".onion") {
        peer
    } else {
        match crate::protocol::friend_code::friend_code_to_onion(&peer) {
            Ok(onion) => onion,
            Err(_) => {
                return RpcResponse::error(id, -32602, "Invalid peer address or friend code".into())
            }
        }
    };

    // Do all DB work under the lock, build the wire message, then release
    // the lock before attempting the async pool.send().
    let (wire_msg, message_id, pool) = {
        let app = app.lock().await;

        // Find friend and conversation
        let friend_id = match crate::db::queries::find_friend_by_onion(&app.db, &peer_onion) {
            Ok(Some(fid)) => fid,
            Ok(None) => return RpcResponse::error(id, -32000, "Peer is not a friend".into()),
            Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
        };
        let conv_id = match crate::db::queries::get_or_create_conversation(&app.db, friend_id) {
            Ok(cid) => cid,
            Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
        };

        let own_onion = app.onion_address.as_deref().unwrap_or("").to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let msg_id = uuid::Uuid::new_v4();

        let ttl =
            crate::db::queries::get_conversation_ephemeral_ttl(&app.db, conv_id).unwrap_or(None);

        // Store locally
        crate::db::queries::store_outgoing_message_with_ttl(
            &app.db,
            conv_id,
            &own_onion,
            &message,
            &msg_id.to_string(),
            ttl,
        )
        .ok();

        // Encrypt with Signal Protocol
        let store = crate::crypto::SessionStore::new(&app.db);
        match store.load_session(&peer_onion) {
            Ok(Some(mut session)) => {
                let payload = crate::protocol::message::PlaintextPayload {
                    content: message.clone(),
                    sent_at: now,
                    message_type: "text".to_string(),
                    ephemeral_ttl: ttl,
                };
                let plaintext = match serde_json::to_vec(&payload) {
                    Ok(p) => p,
                    Err(e) => {
                        return RpcResponse::error(id, -32000, format!("Serialize error: {}", e))
                    }
                };

                let (header, ciphertext, is_prekey) = match session.encrypt(&plaintext) {
                    Ok(result) => result,
                    Err(e) => {
                        return RpcResponse::error(id, -32000, format!("Encrypt error: {}", e))
                    }
                };
                if let Err(e) = store.store_session(&session) {
                    return RpcResponse::error(id, -32000, format!("Session store error: {}", e));
                }

                use base64::Engine;
                let msg = crate::protocol::message::Message::TextMessage(
                    crate::protocol::message::TextMessage {
                        from_onion: own_onion,
                        to_onion: peer_onion.clone(),
                        signal_header: base64::engine::general_purpose::STANDARD.encode(&header),
                        signal_ciphertext: base64::engine::general_purpose::STANDARD
                            .encode(&ciphertext),
                        signal_type: if is_prekey {
                            crate::protocol::message::SignalMessageType::PrekeyMessage
                        } else {
                            crate::protocol::message::SignalMessageType::Message
                        },
                        timestamp: now,
                        message_id: msg_id,
                        x3dh_init: None,
                    },
                );

                let pool = app.connection_pool.as_ref().map(Arc::clone);
                (msg, msg_id, pool)
            }
            Ok(None) => {
                return RpcResponse::error(
                    id,
                    -32000,
                    format!("No encryption session with {}", peer_onion),
                )
            }
            Err(e) => return RpcResponse::error(id, -32000, format!("Session load error: {}", e)),
        }
    };
    // Lock released here

    // Try direct send (async, no lock held)
    if let Some(pool) = pool {
        if pool.send(&peer_onion, &wire_msg).await.is_ok() {
            return RpcResponse::success(
                id,
                serde_json::json!({
                    "status": "sent",
                    "message_id": message_id.to_string(),
                }),
            );
        }
    }

    // Queue for later delivery (needs lock briefly)
    {
        let app = app.lock().await;
        app.message_queue
            .enqueue(&app.db, &peer_onion, &wire_msg, "normal")
            .ok();
    }
    RpcResponse::success(
        id,
        serde_json::json!({
            "status": "queued",
            "message_id": message_id.to_string(),
        }),
    )
}

pub(super) async fn handle_recv_messages(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let peer_filter = params
        .get("peer")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let app = app.lock().await;
    let friends = crate::db::queries::get_friends_with_unread(&app.db).unwrap_or_default();
    let mut all_messages: Vec<Value> = Vec::new();

    for f in &friends {
        if let Some(ref peer) = peer_filter {
            let peer_onion = if peer.ends_with(".onion") {
                peer.clone()
            } else {
                crate::protocol::friend_code::friend_code_to_onion(peer).unwrap_or_default()
            };
            if f.onion_address != peer_onion {
                continue;
            }
        }

        let conv_id =
            crate::db::queries::get_or_create_conversation(&app.db, f.friend_id).unwrap_or(0);
        if conv_id == 0 {
            continue;
        }

        let messages =
            crate::db::queries::get_messages(&app.db, conv_id, limit, 0).unwrap_or_default();
        for m in &messages {
            all_messages.push(serde_json::json!({
                "id": m.id,
                "from": m.sender_onion,
                "message": m.content,
                "timestamp": m.timestamp,
                "status": m.status,
            }));
        }
    }

    RpcResponse::success(id, Value::Array(all_messages))
}

pub(super) async fn handle_ephemeral_set(
    id: Option<Value>,
    app: &Arc<Mutex<App>>,
    params: &Value,
) -> RpcResponse {
    let peer = match params.get("peer").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return RpcResponse::error(id, -32602, "Missing 'peer' parameter".into()),
    };
    let ttl = params.get("ttl").and_then(|v| v.as_i64());

    // Resolve peer
    let peer_onion = if peer.ends_with(".onion") {
        peer
    } else {
        match crate::protocol::friend_code::friend_code_to_onion(&peer) {
            Ok(onion) => onion,
            Err(_) => {
                return RpcResponse::error(id, -32602, "Invalid peer address or friend code".into())
            }
        }
    };

    let app = app.lock().await;
    let friend_id = match crate::db::queries::find_friend_by_onion(&app.db, &peer_onion) {
        Ok(Some(fid)) => fid,
        Ok(None) => return RpcResponse::error(id, -32000, "Peer is not a friend".into()),
        Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
    };
    let conv_id = match crate::db::queries::get_or_create_conversation(&app.db, friend_id) {
        Ok(cid) => cid,
        Err(e) => return RpcResponse::error(id, -32000, format!("{}", e)),
    };

    match crate::db::queries::set_conversation_ephemeral_ttl(&app.db, conv_id, ttl) {
        Ok(()) => RpcResponse::success(
            id,
            serde_json::json!({
                "status": "ok",
                "ttl": ttl,
            }),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("{}", e)),
    }
}
