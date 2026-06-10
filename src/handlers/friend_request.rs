use crate::app::App;
use crate::db;
use crate::error;
use crate::error::Result;
use crate::protocol;
use base64::Engine;

use super::messaging::try_send_direct;

/// Result of attempting to send a message
pub enum SendResult {
    SentImmediately,
    Queued,
}

/// Handle sending a friend request
pub async fn handle_send_friend_request(app: &App, peer_input: &str) -> Result<SendResult> {
    use crate::protocol::friend_request::FriendRequestHandler;

    let trimmed = peer_input.trim();

    // Accept both .onion addresses and friend codes (word sequences)
    let peer_onion = if trimmed.ends_with(".onion") {
        trimmed.to_string()
    } else {
        // Try to decode as a friend code (reversible word encoding of .onion)
        match crate::protocol::friend_code::friend_code_to_onion(trimmed) {
            Ok(onion) => onion,
            Err(_) => {
                return Err(error::ChattorError::Tor(
                    "Enter a .onion address or friend code (word sequence from their Identity)"
                        .into(),
                ))
            }
        }
    };
    let peer_onion = peer_onion.as_str();

    // Get our .onion address
    let own_onion = app
        .onion_address
        .as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Tor not initialized yet".into()))?;

    // Generate our own friend code to include in the request
    let own_friend_code = crate::tor::address::onion_to_friend_code(own_onion)
        .unwrap_or_else(|_| "unknown".to_string());

    // Create friend request message
    let identity = app
        .identity
        .as_ref()
        .ok_or_else(|| error::ChattorError::Crypto("Identity not initialized".into()))?;
    let request_msg = FriendRequestHandler::create_request(identity, own_onion, &own_friend_code)?;

    // Wrap in Message enum
    let message = protocol::message::Message::FriendRequest(request_msg);

    // Try direct send, queue on failure
    match try_send_direct(app, peer_onion, &message).await {
        Ok(_) => Ok(SendResult::SentImmediately),
        Err(_) => {
            // Queue for background delivery
            app.message_queue
                .enqueue(&app.db, peer_onion, &message, "high")?;
            Ok(SendResult::Queued)
        }
    }
}

/// Handle accepting a friend request
pub fn handle_accept_friend_request(app: &App, request_id: i64) -> Result<()> {
    // Get our .onion address
    let own_onion = app
        .onion_address
        .as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Tor not initialized yet".into()))?;

    // Get the friend request from database
    let conn = app.db.connection();
    let (from_onion, _friend_code): (String, String) = conn
        .query_row(
            "SELECT from_onion, friend_code FROM friend_requests WHERE id = ?1",
            [request_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| error::ChattorError::Database(format!("Failed to load request: {}", e)))?;

    let identity = app
        .identity
        .as_ref()
        .ok_or_else(|| error::ChattorError::Crypto("Identity not initialized".into()))?;

    // Generate PreKey bundle and persist all private material via the facade.
    let bundle = crate::crypto::SessionManager::new(&app.db).create_accept_bundle(&from_onion)?;

    // Create accept message (inline to avoid Database clone issue)
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Sign message
    let data = format!("{}{}{}", own_onion, from_onion, timestamp);
    let signature = identity.sign(data.as_bytes());

    // Serialize bundle to JSON
    let bundle_json = serde_json::to_string(&bundle)
        .map_err(|e| error::ChattorError::Crypto(format!("Failed to serialize bundle: {}", e)))?;

    let accept_msg = protocol::message::FriendRequestAcceptMessage {
        from_onion: own_onion.to_string(),
        to_onion: from_onion.clone(),
        signal_prekey_bundle: bundle_json,
        timestamp,
        signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        ed25519_pubkey: Some(identity.public_key_base64()),
    };

    // Add friend to database
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Mark request as accepted FIRST (so UI updates immediately)
    conn.execute(
        "UPDATE friend_requests SET status = 'accepted' WHERE id = ?1",
        [request_id],
    )
    .map_err(|e| error::ChattorError::Database(format!("Failed to update request: {}", e)))?;

    // Use a truncated display name that's more readable
    let display_name = crate::ui::input::truncate_display(&from_onion, 16);

    conn.execute(
        "INSERT OR IGNORE INTO friends (onion_address, display_name, added_at, status)
         VALUES (?1, ?2, ?3, 'active')",
        (&from_onion, &display_name, timestamp),
    )
    .map_err(|e| error::ChattorError::Database(format!("Failed to add friend: {}", e)))?;

    // Auto-subscribe to their channels
    db::queries::add_channel_subscription(&app.db, &from_onion, "public")?;
    db::queries::add_channel_subscription(&app.db, &from_onion, "friends_only")?;

    // Queue the accept message for background delivery (don't try direct send —
    // it can block the UI for up to 30s waiting for a Tor circuit)
    let message = protocol::message::Message::FriendRequestAccept(accept_msg);
    app.message_queue
        .enqueue(&app.db, &from_onion, &message, "high")?;
    eprintln!(
        "Friend request #{} accepted (queued for delivery)",
        request_id
    );

    Ok(())
}

/// Handle rejecting a friend request
pub fn handle_reject_friend_request(app: &App, request_id: i64) -> Result<()> {
    // Simply delete the request from the database
    let conn = app.db.connection();

    let rows_affected = conn
        .execute("DELETE FROM friend_requests WHERE id = ?1", [request_id])
        .map_err(|e| error::ChattorError::Database(format!("Failed to delete request: {}", e)))?;

    if rows_affected == 0 {
        eprintln!("Friend request #{} not found", request_id);
    } else {
        eprintln!("Friend request #{} rejected", request_id);
    }

    Ok(())
}

/// Handle incoming friend request accept
pub fn handle_incoming_accept(
    app: &App,
    accept: &protocol::message::FriendRequestAcceptMessage,
) -> Result<()> {
    use crate::crypto::PreKeyBundle;

    // Deserialize the remote peer's PreKey bundle from the accept message
    let bundle: PreKeyBundle = serde_json::from_str(&accept.signal_prekey_bundle).map_err(|e| {
        error::ChattorError::Crypto(format!("Failed to parse PreKey bundle: {}", e))
    })?;

    // Verify Ed25519 signature on accept message (TOFU)
    if let Some(ref pubkey_b64) = accept.ed25519_pubkey {
        let data = format!(
            "{}{}{}",
            accept.from_onion, accept.to_onion, accept.timestamp
        );
        let pubkey_bytes = base64::engine::general_purpose::STANDARD
            .decode(pubkey_b64)
            .map_err(|e| {
                error::ChattorError::Crypto(format!("Failed to decode accept pubkey: {}", e))
            })?;
        let pubkey_array: [u8; 32] = pubkey_bytes
            .try_into()
            .map_err(|_| error::ChattorError::Crypto("Accept pubkey has wrong length".into()))?;
        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(&accept.signature)
            .map_err(|e| {
                error::ChattorError::Crypto(format!("Failed to decode accept signature: {}", e))
            })?;
        let sig_array: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| error::ChattorError::Crypto("Accept signature has wrong length".into()))?;

        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let verifying_key = VerifyingKey::from_bytes(&pubkey_array)
            .map_err(|_| error::ChattorError::Crypto("Invalid Ed25519 pubkey in accept".into()))?;
        let signature = Signature::from_bytes(&sig_array);
        verifying_key
            .verify(data.as_bytes(), &signature)
            .map_err(|_| {
                error::ChattorError::Crypto(format!(
                    "Accept message from {} has invalid Ed25519 signature",
                    accept.from_onion
                ))
            })?;
    } else {
        return Err(error::ChattorError::Crypto(format!(
            "Accept message from {} missing Ed25519 pubkey, rejecting",
            accept.from_onion
        )));
    }

    // Verify VXEdDSA bundle signature, establish session, and encrypt the handshake
    // PreKey message via the facade.
    let own_onion = app
        .onion_address
        .as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Tor not initialized".into()))?;
    let hs = crate::crypto::SessionManager::new(&app.db)
        .establish_from_accept(&accept.from_onion, &bundle)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let handshake_msg = protocol::message::Message::TextMessage(protocol::message::TextMessage {
        from_onion: own_onion.clone(),
        to_onion: accept.from_onion.clone(),
        signal_header: base64::engine::general_purpose::STANDARD.encode(&hs.header),
        signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&hs.ciphertext),
        signal_type: if hs.is_prekey {
            protocol::message::SignalMessageType::PrekeyMessage
        } else {
            protocol::message::SignalMessageType::Message
        },
        timestamp: now,
        message_id: uuid::Uuid::new_v4(),
        x3dh_init: hs.x3dh_init,
    });

    app.message_queue
        .enqueue(&app.db, &accept.from_onion, &handshake_msg, "high")?;
    eprintln!("Queued handshake PreKey message to {}", accept.from_onion);

    // Add as friend
    let conn = app.db.connection();
    conn.execute(
        "INSERT OR IGNORE INTO friends (onion_address, display_name, added_at, status)
         VALUES (?1, ?2, ?3, 'active')",
        (
            &accept.from_onion,
            &crate::ui::input::truncate_display(&accept.from_onion, 16),
            accept.timestamp,
        ),
    )
    .map_err(|e| error::ChattorError::Database(format!("Failed to add friend: {}", e)))?;

    // Auto-subscribe to their channels
    db::queries::add_channel_subscription(&app.db, &accept.from_onion, "public")?;
    db::queries::add_channel_subscription(&app.db, &accept.from_onion, "friends_only")?;

    // Also subscribe them to our friends_only channel
    db::queries::add_channel_subscriber(&app.db, &accept.from_onion, "friends_only")?;

    // Store peer's Ed25519 pubkey for future verification (TOFU)
    if let Some(ref pubkey_b64) = accept.ed25519_pubkey {
        if let Ok(pubkey_bytes) = base64::engine::general_purpose::STANDARD.decode(pubkey_b64) {
            db::queries::store_friend_pubkey(&app.db, &accept.from_onion, &pubkey_bytes)?;
        }
    }

    eprintln!("Friend request accepted by {}", accept.from_onion);

    Ok(())
}
