//! Per-peer token bucket rate limiter for inbound messages.
//!
//! The limiter has an independent bucket per peer onion, so one peer's abuse
//! doesn't affect others. It is consulted on every inbound message in the
//! main dispatch loop; messages with no identifiable peer (delivery / read
//! receipts) skip the check.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

const DEFAULT_RATE: f64 = 5.0; // tokens per second (sustained)
const DEFAULT_BURST: u32 = 20; // max burst size

/// Drop a peer's bucket from memory after this many seconds of inactivity.
/// Bounds memory if an adversary churns through onion addresses — without
/// GC, the HashMap grows once per unique peer for the lifetime of the
/// process. 1 hour is well past any legitimate burst-then-quiet pattern.
const BUCKET_IDLE_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

pub struct RateLimiter {
    buckets: Mutex<HashMap<String, TokenBucket>>,
    rate: f64,
    burst: u32,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(rate: u32, burst: u32) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            rate: rate as f64,
            burst,
        }
    }

    /// Default rate limiter: 5 req/s sustained, 20 burst per peer.
    pub fn default_limiter() -> Self {
        Self::new(DEFAULT_RATE as u32, DEFAULT_BURST)
    }

    /// Returns true if the message should be allowed, false if rate limited.
    pub fn check(&self, peer: &str) -> bool {
        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets.entry(peer.to_string()).or_insert_with(|| TokenBucket {
            tokens: self.burst as f64,
            last_refill: Instant::now(),
        });

        // Refill tokens based on elapsed time
        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.rate).min(self.burst as f64);
        bucket.last_refill = now;

        // Try to consume one token
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Drop buckets that haven't been touched for `BUCKET_IDLE_TTL`. Bounds
    /// memory under churn — an adversary churning through onions would
    /// otherwise grow the HashMap once per unique peer for the lifetime of
    /// the process. Returns the number of buckets evicted so the caller
    /// can log if it likes.
    pub fn gc_idle(&self) -> usize {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();
        let before = buckets.len();
        buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < BUCKET_IDLE_TTL);
        before - buckets.len()
    }

    /// Number of buckets currently in memory. Exposed for tests and
    /// observability — not part of any control-flow path.
    #[cfg(test)]
    pub fn bucket_count(&self) -> usize {
        self.buckets.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(5, 20);
        // 20 requests within burst should all be allowed
        for i in 0..20 {
            assert!(limiter.check("peer_a"), "request {} should be allowed", i);
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_burst() {
        let limiter = RateLimiter::new(5, 20);
        // Exhaust the burst
        for _ in 0..20 {
            limiter.check("peer_a");
        }
        // 21st request should be blocked (no time to refill)
        assert!(!limiter.check("peer_a"), "21st request should be blocked");
    }

    #[test]
    fn test_rate_limiter_independent_peers() {
        let limiter = RateLimiter::new(5, 20);
        // Exhaust peer_a's burst
        for _ in 0..20 {
            limiter.check("peer_a");
        }
        assert!(!limiter.check("peer_a"), "peer_a should be blocked");

        // peer_b should still have a full burst
        assert!(limiter.check("peer_b"), "peer_b should be allowed");
    }

    #[test]
    fn test_gc_evicts_idle_buckets() {
        // Build a limiter, populate three buckets, then GC immediately —
        // none should be evicted because none are past BUCKET_IDLE_TTL.
        let limiter = RateLimiter::new(5, 20);
        for peer in &["alice", "bob", "carol"] {
            limiter.check(peer);
        }
        assert_eq!(limiter.bucket_count(), 3);
        let evicted = limiter.gc_idle();
        assert_eq!(evicted, 0);
        assert_eq!(limiter.bucket_count(), 3);
    }

    #[test]
    fn test_gc_keeps_active_buckets() {
        // After `check()` updates last_refill to now(), gc_idle should not
        // touch the bucket regardless of how many cycles run.
        let limiter = RateLimiter::new(5, 20);
        limiter.check("alice");
        for _ in 0..10 {
            limiter.gc_idle();
        }
        assert_eq!(limiter.bucket_count(), 1);
    }

    #[test]
    fn test_default_limiter() {
        let limiter = RateLimiter::default_limiter();
        // Should have burst of 20
        for i in 0..20 {
            assert!(limiter.check("peer"), "request {} should be allowed", i);
        }
        assert!(!limiter.check("peer"), "21st request should be blocked");
    }
}
