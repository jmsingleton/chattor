---
title: Friend Codes
description: How chattor's 32-word friend codes work
---

## What Are Friend Codes?

Friend codes are chattor's way of exchanging identities without a central server. Each code is a **32-word mnemonic** that encodes your public key, similar to how Bitcoin seed phrases work.

A friend code looks like this:

```
alpine bridge castle dragon ember falcon glacier harbor
island jasper kindle lantern meadow nebula orchid prism
quartz river sunset tower umbra velvet whisper xenon
```

(8 groups of 4 words, drawn from a 256-word dictionary)

## How They Work

1. Your Ed25519 public key is encoded as a sequence of 32 words from a fixed dictionary
2. The encoding is deterministic — the same key always produces the same code
3. Friend codes map to `.onion` addresses via a SHA-256 hash
4. The mapping is one-way: you can derive the `.onion` from the code, but not the code from the `.onion`

## Sharing Your Code

1. Press **`[i]`** to open the Identity modal
2. Press **`[c]`** to copy your friend code to the clipboard
3. Share it via any out-of-band channel (in person, encrypted message, etc.)

## Adding a Friend

1. Press **`[a]`** to open the Add Friend modal
2. Paste or type your friend's 32-word code
3. A friend request is sent over Tor to their hidden service
4. Once they accept, a Signal Protocol session is established via X3DH key exchange

## Security Considerations

- Friend codes should be exchanged over a **trusted channel** — they are your identity
- Anyone with your friend code can send you a friend request
- You can reject unwanted friend requests
- The friend code itself does not reveal your `.onion` address to a passive observer (SHA-256 is one-way)
