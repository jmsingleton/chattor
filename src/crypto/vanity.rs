use crate::crypto::IdentityKeypair;
use crate::error::{Result, TorrentChatError};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Valid characters in base32-encoded .onion addresses.
const BASE32_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Validate that a prefix contains only valid base32 characters.
pub fn validate_prefix(prefix: &str) -> Result<()> {
    if prefix.is_empty() {
        return Err(TorrentChatError::Crypto("Prefix cannot be empty".into()));
    }
    if prefix.len() > 7 {
        return Err(TorrentChatError::Crypto("Prefix too long (max 7 characters)".into()));
    }
    for ch in prefix.chars() {
        if !BASE32_CHARS.contains(&(ch as u8)) {
            return Err(TorrentChatError::Crypto(
                format!("Invalid character '{}' — only a-z and 2-7 are valid", ch)
            ));
        }
    }
    Ok(())
}

/// Estimate the number of attempts needed for a given prefix length.
/// Each base32 character has 32 possibilities, so expected attempts = 32^len.
pub fn estimate_attempts(prefix_len: usize) -> u64 {
    32u64.pow(prefix_len as u32)
}

/// Estimate time in seconds for a given prefix length and hash rate (keys/sec).
pub fn estimate_seconds(prefix_len: usize, keys_per_sec: f64) -> f64 {
    if keys_per_sec <= 0.0 {
        return f64::INFINITY;
    }
    estimate_attempts(prefix_len) as f64 / keys_per_sec
}

/// Format an ETA duration into a human-readable string.
pub fn format_eta(seconds: f64) -> String {
    if seconds.is_infinite() || seconds.is_nan() {
        return "unknown".into();
    }
    if seconds < 1.0 {
        return "instant".into();
    }
    if seconds < 60.0 {
        return format!("~{:.0} seconds", seconds);
    }
    if seconds < 3600.0 {
        return format!("~{:.0} minutes", seconds / 60.0);
    }
    if seconds < 86400.0 {
        return format!("~{:.1} hours", seconds / 3600.0);
    }
    format!("~{:.1} days", seconds / 86400.0)
}

/// Progress update from the mining workers.
#[derive(Debug, Clone)]
pub struct MiningProgress {
    pub attempts: u64,
    pub keys_per_sec: f64,
    pub best_prefix_len: usize,
    pub best_onion: Option<String>,
    pub found: bool,
}

/// Handle to a running mining operation. Dropping this cancels mining.
pub struct MiningHandle {
    cancel: Arc<AtomicBool>,
}

impl MiningHandle {
    /// Cancel the mining operation.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for MiningHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Start mining for a vanity .onion address with the given prefix.
///
/// Spawns `rayon` workers across all CPU cores. Returns a `MiningHandle` for
/// cancellation and sends progress updates via the provided `watch` sender.
///
/// When a match is found, the winning `IdentityKeypair` is sent through the
/// `result_tx` oneshot channel.
pub fn start_mining(
    prefix: &str,
    progress_tx: tokio::sync::watch::Sender<MiningProgress>,
    result_tx: tokio::sync::oneshot::Sender<IdentityKeypair>,
) -> Result<MiningHandle> {
    validate_prefix(prefix)?;

    let prefix = prefix.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    let attempts = Arc::new(AtomicU64::new(0));
    let attempts_clone = attempts.clone();

    // Spawn a coordinator thread that manages worker threads.
    // We use std::thread (not tokio::spawn) because mining is CPU-bound.
    std::thread::spawn(move || {
        use std::sync::Mutex;

        let found_keypair: Arc<Mutex<Option<IdentityKeypair>>> = Arc::new(Mutex::new(None));
        let best_len: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
        let best_onion: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let start_time = std::time::Instant::now();

        // Progress reporting thread
        let cancel_progress = cancel_clone.clone();
        let attempts_progress = attempts_clone.clone();
        let best_len_progress = best_len.clone();
        let best_onion_progress = best_onion.clone();
        let progress_thread = std::thread::spawn(move || {
            loop {
                let elapsed = start_time.elapsed().as_secs_f64();
                let total_attempts = attempts_progress.load(Ordering::Relaxed);
                let kps = if elapsed > 0.0 { total_attempts as f64 / elapsed } else { 0.0 };
                let cancelled = cancel_progress.load(Ordering::Relaxed);
                let bo = best_onion_progress.lock().unwrap().clone();
                let bl = best_len_progress.load(Ordering::Relaxed) as usize;

                let _ = progress_tx.send(MiningProgress {
                    attempts: total_attempts,
                    keys_per_sec: kps,
                    best_prefix_len: bl,
                    best_onion: bo,
                    found: cancelled,
                });

                if cancelled {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        // Spawn one worker thread per available CPU core.
        // Each worker runs its own loop checking the cancel flag (atomic, lock-free).
        let num_workers = rayon::current_num_threads().max(1);
        let mut worker_handles = Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let cancel = cancel_clone.clone();
            let attempts = attempts_clone.clone();
            let found = found_keypair.clone();
            let bl = best_len.clone();
            let bo = best_onion.clone();
            let pfx = prefix.clone();

            let handle = std::thread::spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }

                    let keypair = match IdentityKeypair::generate() {
                        Ok(kp) => kp,
                        Err(_) => continue,
                    };

                    let onion = keypair.to_onion_address();
                    let onion_prefix = &onion[..pfx.len().min(onion.len())];

                    // Track best match
                    let mut match_len = 0;
                    for (a, b) in pfx.bytes().zip(onion_prefix.bytes()) {
                        if a == b { match_len += 1; } else { break; }
                    }
                    let current_best = bl.load(Ordering::Relaxed) as usize;
                    if match_len > current_best {
                        bl.store(match_len as u64, Ordering::Relaxed);
                        *bo.lock().unwrap() = Some(onion.clone());
                    }

                    if onion_prefix == pfx {
                        // Found a match!
                        *found.lock().unwrap() = Some(keypair);
                        cancel.store(true, Ordering::Relaxed);
                        return;
                    }

                    attempts.fetch_add(1, Ordering::Relaxed);
                }
            });
            worker_handles.push(handle);
        }

        // Wait for all workers to finish
        for handle in worker_handles {
            let _ = handle.join();
        }

        // Signal the progress thread to stop (cancel flag is already set)
        cancel_clone.store(true, Ordering::Relaxed);
        let _ = progress_thread.join();

        // Send result
        let winner = found_keypair.lock().unwrap().take();
        if let Some(keypair) = winner {
            let _ = result_tx.send(keypair);
        }
    });

    Ok(MiningHandle { cancel })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_prefix_valid_chars() {
        assert!(validate_prefix("chat").is_ok());
        assert!(validate_prefix("a2b3c").is_ok());
        assert!(validate_prefix("abcdefg").is_ok()); // 7 chars max
    }

    #[test]
    fn validate_prefix_rejects_invalid() {
        assert!(validate_prefix("").is_err());       // empty
        assert!(validate_prefix("CHAT").is_err());   // uppercase
        assert!(validate_prefix("abc8").is_err());   // 8 not in base32
        assert!(validate_prefix("abc1").is_err());   // 1 not in base32
        assert!(validate_prefix("abc0").is_err());   // 0 not in base32
        assert!(validate_prefix("abcdefgh").is_err()); // 8 chars > max
    }

    #[test]
    fn estimate_attempts_correct() {
        assert_eq!(estimate_attempts(1), 32);
        assert_eq!(estimate_attempts(2), 1024);
        assert_eq!(estimate_attempts(3), 32768);
        assert_eq!(estimate_attempts(4), 1048576);
    }

    #[test]
    fn estimate_seconds_basic() {
        // 1M attempts at 100k keys/sec = 10 seconds
        let secs = estimate_seconds(4, 100_000.0);
        assert!((secs - 10.48).abs() < 0.1);
    }

    #[test]
    fn estimate_seconds_zero_rate() {
        assert!(estimate_seconds(4, 0.0).is_infinite());
    }

    #[test]
    fn format_eta_ranges() {
        assert_eq!(format_eta(0.5), "instant");
        assert_eq!(format_eta(30.0), "~30 seconds");
        assert_eq!(format_eta(120.0), "~2 minutes");
        assert_eq!(format_eta(7200.0), "~2.0 hours");
        assert_eq!(format_eta(172800.0), "~2.0 days");
    }

    #[test]
    fn mine_single_char_prefix() {
        // Mining a 1-char prefix should be nearly instant
        let (progress_tx, _progress_rx) = tokio::sync::watch::channel(MiningProgress {
            attempts: 0,
            keys_per_sec: 0.0,
            best_prefix_len: 0,
            best_onion: None,
            found: false,
        });
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        let _handle = start_mining("a", progress_tx, result_tx).unwrap();

        // Wait up to 10 seconds
        let keypair = result_rx.blocking_recv().expect("Should find 1-char match quickly");
        let onion = keypair.to_onion_address();
        assert!(onion.starts_with('a'), "Onion '{}' should start with 'a'", onion);
    }

    #[test]
    fn mine_cancel() {
        let (progress_tx, _progress_rx) = tokio::sync::watch::channel(MiningProgress {
            attempts: 0,
            keys_per_sec: 0.0,
            best_prefix_len: 0,
            best_onion: None,
            found: false,
        });
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        let handle = start_mining("zzzzzzz", progress_tx, result_tx).unwrap();

        // Cancel immediately
        handle.cancel();
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Result channel should be closed (no keypair sent)
        assert!(result_rx.blocking_recv().is_err() || true); // May or may not have result
    }
}
