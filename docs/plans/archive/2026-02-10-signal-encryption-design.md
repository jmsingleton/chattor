# Signal Protocol Encryption for Text Messages - Design Document

**Date:** 2026-02-10
**Status:** Approved
**Phase:** Wire up real encryption

## Overview

Wire up the existing Signal Protocol infrastructure (MessageSender, MessageReceiver, real X3DH sessions) so text messages are actually encrypted end-to-end. Currently the `signal_ciphertext` field contains raw plaintext.

**Goal:** Text messages are encrypted with ChaCha20Poly1305 via Signal sessions before sending, and decrypted on receipt.

**Architecture:** Three changes in main.rs — upgrade session creation to real X3DH, use MessageSender::prepare_message() for sending, use MessageReceiver::decrypt_message() for receiving.

---

## Change 1: Session Creation

Both `handle_accept_friend_request` and `handle_incoming_accept` currently call the MVP stub `SignalSession::from_prekey_bundle()`. Upgrade to `from_prekey_bundle_real()` which performs actual X25519 Diffie-Hellman key exchange.

- Acceptor: Already generates `(bundle, private_keys)` via `PreKeyBundle::generate_real()`. Pass `private_keys` and identity into real session creation.
- Initiator: Receives bundle in accept message. Call `from_prekey_bundle_real()` with received bundle and own identity.

## Change 2: Send Path

Replace manual TextMessage construction in the SendMessage handler with `MessageSender::prepare_message()`. This:
1. Loads Signal session from DB
2. Creates PlaintextPayload, encrypts with ChaCha20Poly1305
3. Base64-encodes ciphertext, sets signal_type
4. Updates session in DB (counter increment)

Store plaintext locally first (for our own display), then send the encrypted message. On send failure, enqueue the already-encrypted message.

## Change 3: Receive Path

Replace raw plaintext read in the TextMessage handler with `MessageReceiver::decrypt_message()`. This:
1. Loads Signal session for sender
2. Decrypts ciphertext
3. Returns PlaintextPayload with original content

Store the decrypted content in the conversation. Handle decryption failures gracefully (log and skip).

## Success Criteria

1. Existing tests pass
2. signal_ciphertext contains real encrypted data
3. Messages round-trip: encrypt → decrypt → original content
4. Failed decryption handled gracefully
5. Session counters prevent nonce reuse
