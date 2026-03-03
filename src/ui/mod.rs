pub mod app_ui;
pub mod bootstrap;
pub mod channel_feed;
pub mod conversation;
pub mod error;
pub mod input;
pub mod modals;
pub mod sidebar;
pub mod state;
pub mod theme;

pub use app_ui::{render_app, RenderContext};
pub use bootstrap::{
    handle_bootstrap_key, render_connecting, render_failure, BootstrapAction, BootstrapPhase,
    BootstrapUpdate,
};
pub use state::{AppAction, AppState};
pub use theme::Theme;

use crate::app::App;

/// Display connection status
#[allow(dead_code)]
pub fn display_connection_status(app: &App) {
    println!("\n=== Connection Status ===");

    // Tor status
    match &app.tor_client {
        Some(_) => println!("Tor: ✓ Connected"),
        None => println!("Tor: ✗ Not connected"),
    }

    // Onion address
    match &app.onion_address {
        Some(addr) => println!("Onion Address: {}", addr),
        None => println!("Onion Address: (not set)"),
    }

    // Hidden service
    match &app.hidden_service {
        Some(hs) => println!("Hidden Service: ✓ Running on {}", hs.address()),
        None => println!("Hidden Service: ✗ Not running"),
    }

    // Message queue
    println!("Message Queue: Active");

    println!("========================\n");
}

/// Copy text to system clipboard. Returns true on success.
///
/// On Linux, CLI tools (wl-copy/xclip/xsel) are tried first because arboard's
/// clipboard contents die when the Clipboard struct is dropped. CLI tools fork
/// a background process that persists the contents independently.
#[allow(clippy::needless_return)]
pub fn copy_to_clipboard(text: &str) -> bool {
    // On Linux, try CLI tools first — they persist clipboard contents
    // independently of our process, avoiding the "dropped too quickly" issue.
    #[cfg(target_os = "linux")]
    {
        if clipboard_fallback(text) {
            return true;
        }
    }

    // Try arboard. On Linux/Wayland, spawn a background thread with wait()
    // so the clipboard selection is served until replaced (up to 30s).
    // We use a channel to know if the set actually succeeded.
    #[cfg(target_os = "linux")]
    {
        let text_owned = text.to_owned();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> std::result::Result<(), arboard::Error> {
                let mut clipboard = arboard::Clipboard::new()?;
                use arboard::SetExtLinux;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                clipboard.set().wait_until(deadline).text(text_owned)?;
                Ok(())
            })();
            let _ = tx.send(result.is_ok());
        });
        // Wait briefly for the clipboard to be set
        if let Ok(true) = rx.recv_timeout(std::time::Duration::from_millis(500)) {
            return true;
        }
        // If arboard also failed, nothing we can do
        return false;
    }

    // On non-Linux, arboard works fine without wait()
    #[cfg(not(target_os = "linux"))]
    {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(()) = clipboard.set_text(text) {
                return true;
            }
        }
        clipboard_fallback(text)
    }
}

/// Fallback clipboard using command-line tools (wl-copy, xclip, xsel, pbcopy).
fn clipboard_fallback(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let tools: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
    ];

    for (cmd, args) in tools {
        if let Ok(mut child) = Command::new(cmd)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                if stdin.write_all(text.as_bytes()).is_ok() {
                    drop(stdin);
                    if let Ok(status) = child.wait() {
                        if status.success() {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use tempfile::TempDir;

    #[test]
    fn test_display_connection_status() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let app = App::new(None).unwrap();

        // Should not panic
        display_connection_status(&app);
    }
}
