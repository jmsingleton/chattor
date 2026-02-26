use crate::error::{ChattorError, Result};
use std::fs;
use std::path::Path;

/// Acquire a PID file. Returns error if another instance is running.
pub fn acquire(path: &Path) -> Result<()> {
    if path.exists() {
        let contents = fs::read_to_string(path).map_err(ChattorError::Io)?;
        if let Ok(pid) = contents.trim().parse::<u32>() {
            if process_exists(pid) {
                return Err(ChattorError::Tor(format!(
                    "Another chattor instance is running (PID {}). Stop it first or remove {}",
                    pid,
                    path.display()
                )));
            }
        }
        // Stale PID file -- remove it
        fs::remove_file(path).ok();
    }

    fs::write(path, format!("{}", std::process::id())).map_err(ChattorError::Io)?;
    Ok(())
}

/// Release the PID file.
pub fn release(path: &Path) {
    fs::remove_file(path).ok();
}

/// Check if a process with the given PID exists.
fn process_exists(pid: u32) -> bool {
    // On Unix, signal 0 checks existence without actually sending a signal
    // SAFETY: kill(pid, 0) with signal 0 only checks process existence,
    // it does not actually send a signal or modify any state.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_acquire_creates_pid_file() {
        let dir = TempDir::new().unwrap();
        let pid_path = dir.path().join("test.pid");

        acquire(&pid_path).unwrap();
        assert!(pid_path.exists());

        let contents = fs::read_to_string(&pid_path).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(pid, std::process::id());

        release(&pid_path);
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_acquire_removes_stale_pid_file() {
        let dir = TempDir::new().unwrap();
        let pid_path = dir.path().join("test.pid");

        // Write a stale PID (PID 1 is init, but a very high PID likely doesn't exist)
        fs::write(&pid_path, "999999999").unwrap();

        // Should succeed because the PID doesn't exist
        acquire(&pid_path).unwrap();

        let contents = fs::read_to_string(&pid_path).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(pid, std::process::id());

        release(&pid_path);
    }

    #[test]
    fn test_acquire_fails_if_process_running() {
        let dir = TempDir::new().unwrap();
        let pid_path = dir.path().join("test.pid");

        // Write our own PID (which is definitely running)
        fs::write(&pid_path, format!("{}", std::process::id())).unwrap();

        let result = acquire(&pid_path);
        assert!(result.is_err());

        // Clean up
        release(&pid_path);
    }

    #[test]
    fn test_release_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let pid_path = dir.path().join("nonexistent.pid");

        // Should not panic
        release(&pid_path);
    }

    #[test]
    fn test_process_exists_current_process() {
        assert!(process_exists(std::process::id()));
    }

    #[test]
    fn test_process_exists_nonexistent() {
        // Very high PID that almost certainly doesn't exist
        assert!(!process_exists(999_999_999));
    }
}
