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
    use crate::crypto::PreKeyBundle;

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

    // Generate PreKey bundle for the accept message.
    // Generate a dedicated X25519 Signal identity keypair for X3DH.
    // This is separate from the Ed25519 identity used for friend request signing.
    let identity = app
        .identity
        .as_ref()
        .ok_or_else(|| error::ChattorError::Crypto("Identity not initialized".into()))?;
    let signal_identity = libsignal_protocol::vxeddsa::gen_keypair();
    let signal_identity_public_raw =
        libsignal_protocol::utils::decode_public_key(&signal_identity.public).map_err(|_| {
            error::ChattorError::Crypto("Failed to decode signal identity public key".into())
        })?;
    let (bundle, private_keys) =
        PreKeyBundle::generate_real(&signal_identity.secret, &signal_identity_public_raw)?;

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
    };

    // Store PreKey private material so we can create the Signal session later
    // when the peer sends their first PreKey message. We do NOT create the
    // session here — the shared secret requires the peer's ephemeral key,
    // which is embedded in their first encrypted message.
    let identity_b64 =
        base64::engine::general_purpose::STANDARD.encode(private_keys.identity_secret);
    let spk_b64 =
        base64::engine::general_purpose::STANDARD.encode(private_keys.signed_prekey_secret);
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (&format!("prekey_identity:{}", from_onion), &identity_b64),
    )
    .map_err(|e| {
        error::ChattorError::Database(format!("Failed to store PreKey identity material: {}", e))
    })?;
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (&format!("prekey_spk:{}", from_onion), &spk_b64),
    )
    .map_err(|e| {
        error::ChattorError::Database(format!("Failed to store PreKey SPK material: {}", e))
    })?;
    if let Some(opk_secret) = private_keys.prekey_secret {
        let opk_b64 = base64::engine::general_purpose::STANDARD.encode(opk_secret);
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
            (&format!("prekey_opk:{}", from_onion), &opk_b64),
        )
        .map_err(|e| {
            error::ChattorError::Database(format!("Failed to store PreKey OPK material: {}", e))
        })?;
    }
    // Also store the Signal identity secret for the initiator side
    // (needed when handle_incoming_accept creates the session)
    let signal_secret_b64 =
        base64::engine::general_purpose::STANDARD.encode(signal_identity.secret);
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        (
            &format!("signal_identity_secret:{}", from_onion),
            &signal_secret_b64,
        ),
    )
    .map_err(|e| {
        error::ChattorError::Database(format!("Failed to store Signal identity secret: {}", e))
    })?;

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
    use crate::crypto::{PreKeyBundle, PreKeyPrivateMaterial, SessionStore, SignalSession};

    // Deserialize the remote peer's PreKey bundle from the accept message
    let bundle: PreKeyBundle = serde_json::from_str(&accept.signal_prekey_bundle).map_err(|e| {
        error::ChattorError::Crypto(format!("Failed to parse PreKey bundle: {}", e))
    })?;

    // Load our Signal identity secret that was stored when we sent the friend request.
    // We are the original requester; the acceptor sent us their PreKey bundle.
    // We need our Signal identity to perform X3DH as initiator.
    //
    // When we sent the friend request, we didn't have the peer's .onion yet.
    // But when we ACCEPTED a friend request from them (handle_accept_friend_request),
    // we stored signal_identity_secret:<peer_onion>. However, in the case where
    // we are the REQUESTER receiving an accept, we need to generate a new Signal
    // identity now (the requester didn't pre-store one because the accept contains the bundle).
    let signal_identity_secret: [u8; 32] = {
        // Check if we stored a signal identity secret for this peer
        let key = format!("signal_identity_secret:{}", accept.from_onion);
        match app.db.connection().query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [&key],
            |row| row.get::<_, String>(0),
        ) {
            Ok(b64) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(&b64)
                    .map_err(|e| {
                        error::ChattorError::Crypto(format!(
                            "Failed to decode stored Signal identity secret: {}",
                            e
                        ))
                    })?;
                bytes.try_into().map_err(|_| {
                    error::ChattorError::Crypto(
                        "Stored Signal identity secret has wrong length".into(),
                    )
                })?
            }
            Err(_) => {
                // Generate a fresh Signal identity for this X3DH exchange
                let kp = libsignal_protocol::vxeddsa::gen_keypair();
                kp.secret
            }
        }
    };

    let _identity = app
        .identity
        .as_ref()
        .ok_or_else(|| error::ChattorError::Crypto("Identity not initialized".into()))?;
    let dummy_private = PreKeyPrivateMaterial {
        identity_secret: [0u8; 32],
        signed_prekey_secret: [0u8; 32],
        prekey_secret: None,
    };
    let (session, _ad, ephemeral_public) = SignalSession::from_prekey_bundle_real(
        accept.from_onion.clone(),
        &bundle,
        &dummy_private,
        &signal_identity_secret,
    )?;

    // Compute our identity public key for the X3DH init data
    let our_identity_encoded = libsignal_protocol::vxeddsa::gen_pubkey(&signal_identity_secret);

    // Store session
    let store = SessionStore::new(&app.db);
    store.store_session(&session)?;

    // Queue a handshake PreKey message to trigger the peer's session creation.
    // Without this, the acceptor can't send messages because they deferred
    // session creation until our first PreKey message arrives.
    let own_onion = app
        .onion_address
        .as_ref()
        .ok_or_else(|| error::ChattorError::Tor("Tor not initialized".into()))?;
    {
        let mut session = store.load_session(&accept.from_onion)?.ok_or_else(|| {
            error::ChattorError::Crypto("Session just stored but not found".into())
        })?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let handshake = protocol::message::PlaintextPayload {
            content: String::new(),
            sent_at: now,
            message_type: "handshake".to_string(),
            ephemeral_ttl: None,
        };
        let plaintext = serde_json::to_vec(&handshake)
            .map_err(|e| error::ChattorError::Crypto(format!("Handshake serialize: {}", e)))?;

        let (header, ciphertext, is_prekey) = session.encrypt(&plaintext)?;
        store.store_session(&session)?; // persist updated ratchet state

        // Build X3DH init data for the PreKey message so Bob can run x3dh_responder
        let x3dh_init = if is_prekey {
            Some(protocol::message::X3DHInitData {
                sender_identity_key: base64::engine::general_purpose::STANDARD
                    .encode(our_identity_encoded),
                sender_ephemeral_key: base64::engine::general_purpose::STANDARD
                    .encode(ephemeral_public),
            })
        } else {
            None
        };

        let handshake_msg =
            protocol::message::Message::TextMessage(protocol::message::TextMessage {
                from_onion: own_onion.clone(),
                to_onion: accept.from_onion.clone(),
                signal_header: base64::engine::general_purpose::STANDARD.encode(&header),
                signal_ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
                signal_type: if is_prekey {
                    protocol::message::SignalMessageType::PrekeyMessage
                } else {
                    protocol::message::SignalMessageType::Message
                },
                timestamp: now,
                message_id: uuid::Uuid::new_v4(),
                x3dh_init,
            });

        app.message_queue
            .enqueue(&app.db, &accept.from_onion, &handshake_msg, "high")?;
        eprintln!("Queued handshake PreKey message to {}", accept.from_onion);
    }

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

    eprintln!("Friend request accepted by {}", accept.from_onion);

    Ok(())
}
