---
title: FAQ
description: Frequently asked questions about chattor
---

## General

### What is chattor?

A peer-to-peer encrypted chat application that runs in your terminal. Each user hosts their own Tor hidden service — there are no central servers, accounts, or registration.

### What platforms are supported?

Linux, macOS, and BSD. Windows is not supported.

### Does chattor require a system Tor installation?

No. Tor is embedded via the [arti](https://gitlab.torproject.org/tpo/core/arti) library (pure Rust). Everything is bundled in the chattor binary.

### Is chattor anonymous?

chattor routes all traffic through Tor, which hides your IP address from peers. Your identity is your Ed25519 keypair and `.onion` address — no real-world identity is attached.

## Security

### What encryption does chattor use?

Three layers:

1. **End-to-end**: Signal Protocol (X3DH + ChaCha20-Poly1305) between peers
2. **In-transit**: Tor encryption for network transport
3. **At-rest**: SQLCipher for the local database

### Can messages be sent unencrypted?

No. The plaintext fallback was removed entirely. If no Signal Protocol session exists with a peer, messages cannot be sent.

### Are messages stored on any server?

No. Messages are only stored on the sender's and recipient's local machines, in an encrypted SQLCipher database.

## Troubleshooting

### Tor bootstrap is taking a long time

Initial Tor bootstrap can take 30-60 seconds. Subsequent startups are faster because arti caches state. If it takes more than 2 minutes, check your network connection.

### Messages aren't being delivered

If a peer is offline, messages are automatically queued and retried with exponential backoff (up to 24 hours). Check that both peers have active Tor connections.

### How do I reset my identity?

Delete the chattor data directory:

```bash
# Linux
rm -rf ~/.local/share/chattor/

# macOS
rm -rf ~/Library/Application\\ Support/chattor/
```

This deletes your identity, all messages, and all friend relationships. There is no recovery.
