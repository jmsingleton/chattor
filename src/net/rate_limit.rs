use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

const DEFAULT_RATE: f64 = 5.0; // tokens per second (sustained)
const DEFAULT_BURST: u32 = 20; // max burst size

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

    #[allow(dead_code)]
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
}

// TODO: wire into message dispatch in listener.rs or main.rs incoming message handler

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
    fn test_default_limiter() {
        let limiter = RateLimiter::default_limiter();
        // Should have burst of 20
        for i in 0..20 {
            assert!(limiter.check("peer"), "request {} should be allowed", i);
        }
        assert!(!limiter.check("peer"), "21st request should be blocked");
    }
}
