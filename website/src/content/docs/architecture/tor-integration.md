---
title: Tor Integration
description: How chattor uses embedded Tor for anonymous P2P networking
---

## Embedded Tor via Arti

chattor embeds Tor using [arti](https://gitlab.torproject.org/tpo/core/arti), the Tor Project's pure-Rust Tor implementation. This means:

- **No system Tor required** — arti is compiled into the chattor binary
- **Persistent identity** — your `.onion` address survives restarts (arti manages the key in its state directory)
- **Single process** — no separate Tor daemon to manage

## Onion Service

Each chattor user runs a v3 Tor onion service. This is your "server" — other users connect to your `.onion` address to send you messages.

### Lifecycle

1. On startup, `TorClient::new_with_data_dir()` bootstraps the Tor connection using a persistent state directory
2. `HiddenService::launch()` starts the onion service via arti's `launch_onion_service()`
3. The `.onion` address is cached in the database's `app_settings` table for display before Tor connects
4. A listener task accepts incoming Tor rendezvous streams and processes messages

### Address Mapping

Friend codes map to `.onion` addresses via SHA-256:

```
friend_code → SHA-256 hash → .onion address
```

This mapping is deterministic (same code always produces the same address) and one-way (you can't reverse the `.onion` back to the friend code).

## Connection Pool

Building a Tor circuit takes 10-30 seconds. To avoid this per-message latency, chattor maintains a **connection pool** (`src/net/pool.rs`) using DashMap for lock-free concurrent access:

- **Per-peer caching**: One cached circuit per peer, max 50 connections
- **Idle eviction**: Unused connections are dropped after 5 minutes
- **Retry-on-stale**: If a cached circuit is dead, it's evicted and a fresh one is built
- **Timeouts**: 30s for circuit building, 10s for message sending

Inbound messages are also protected by a **per-peer rate limiter** (`src/net/rate_limit.rs`) — a token bucket allowing 5 messages/second sustained with a burst of 20, preventing any single peer from overwhelming the receiver.

## Message Delivery

Messages flow through the connection pool:

1. Check pool for an existing circuit to the peer
2. If found, send the message through the cached circuit
3. If the circuit is dead, evict it and build a new one (retry once)
4. If the peer is unreachable, the message is queued in the offline queue

The **offline message queue** (`src/net/queue.rs`) handles delivery retries:

- FIFO queue persisted in the database
- Exponential backoff: 30s base, doubles each attempt, capped at 15 minutes
- 24-hour expiry window — messages older than 24h are dropped
- Up to 10 peers are processed concurrently via a semaphore
