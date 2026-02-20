---
title: Signal Protocol
description: How chattor implements end-to-end encryption
---

## Overview

chattor implements Signal Protocol using [libsignal-dezire](https://github.com/nicksenger/libsignal-dezire), a pure-Rust Signal Protocol library providing real X3DH key exchange and Double Ratchet message encryption. Key cryptographic components:

- **X3DH** (Extended Triple Diffie-Hellman) for initial key agreement
- **Double Ratchet** for ongoing message encryption with forward secrecy
- **Ed25519** for identity keys and message signing (via `ed25519-dalek`)

## X3DH Key Exchange

When two users establish a conversation, they perform an X3DH handshake to derive a shared secret:

### Session Establishment Flow

1. **Alice sends a friend request** to Bob (Ed25519-signed)
2. **Bob accepts**: generates a PreKeyBundle (identity key + signed prekey + one-time prekey), stores the private material, and queues the accept message
3. **Alice receives the accept**: calls `from_prekey_bundle_real()` with Bob's bundle, derives the shared secret, stores the session, and queues a handshake PreKey message
4. **Bob receives the handshake**: loads stored private material, calls `from_prekey_message_real()` to derive the same shared secret, stores the session, and cleans up private material
5. **Both sides now have established sessions** — bidirectional messaging works

### Key Types

| Key | Purpose |
|-----|---------|
| Identity Key (Ed25519) | Long-term identity, signs prekeys |
| Signed PreKey (X25519) | Medium-term key in PreKeyBundle |
| One-Time PreKey (X25519) | Ephemeral key for forward secrecy |
| Ephemeral Key (X25519) | Per-handshake key from initiator |

## Message Encryption

Once a session is established, messages are encrypted using the **Double Ratchet algorithm** via libsignal-dezire:

1. Plaintext is serialized to bytes
2. The Double Ratchet advances the chain key and derives a message key
3. The message is encrypted with the derived key
4. The ciphertext is wrapped in a `MessageEnvelope` with `signal_header` (ratchet state), `signal_ciphertext` (base64), and `signal_type` (PreKeyMessage or Message)
5. The envelope includes a `protocol_version` field for forward compatibility

## Security Properties

- **Forward secrecy**: Compromising long-term keys doesn't decrypt past messages
- **No plaintext fallback**: If no Signal session exists, encryption fails hard — messages are never sent unencrypted
- **Session persistence**: Signal sessions are stored in the encrypted SQLCipher database
- **Key verification**: Friend requests are Ed25519-signed to prevent impersonation
