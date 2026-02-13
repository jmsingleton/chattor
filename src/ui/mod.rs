pub mod app_ui;
pub mod bootstrap;
pub mod channel_feed;
pub mod conversation;
pub mod modals;
pub mod sidebar;
pub mod state;
pub mod error;
pub mod theme;

pub use app_ui::{render_app, RenderContext};
pub use bootstrap::{
    BootstrapAction, BootstrapPhase, BootstrapUpdate,
    render_connecting, render_failure, handle_bootstrap_key,
};
pub use channel_feed::render_channel_feed;
pub use conversation::{render_conversation, render_input};
pub use modals::{render_add_friend_modal, render_friend_request_modal, render_identity_modal};
pub use sidebar::{render_sidebar, render_sidebar_with_channels};
pub use state::{AppState, AppAction};
pub use error::format_error_for_user;
pub use theme::Theme;

use crate::app::App;

/// Display connection status
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
pub fn copy_to_clipboard(text: &str) -> bool {
    // Try arboard first
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            match clipboard.set_text(text) {
                Ok(()) => return true,
                Err(_) => {} // Fall through to CLI fallback
            }
        }
        Err(_) => {} // Fall through to CLI fallback
    }

    // Fallback: try command-line clipboard tools
    clipboard_fallback(text)
}

/// Fallback clipboard using command-line tools (wl-copy, xclip, xsel, pbcopy).
fn clipboard_fallback(text: &str) -> bool {
    use std::process::{Command, Stdio};
    use std::io::Write;

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

        let app = App::new().unwrap();

        // Should not panic
        display_connection_status(&app);
    }
}
