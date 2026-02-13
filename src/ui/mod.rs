pub mod app_ui;
pub mod bootstrap;
pub mod conversation;
pub mod modals;
pub mod sidebar;
pub mod state;
pub mod error;

pub use app_ui::{render_app, RenderContext};
pub use bootstrap::{BootstrapAction, BootstrapPhase, BootstrapUpdate};
pub use conversation::{render_conversation, render_setup_wizard, render_input};
pub use modals::{render_add_friend_modal, render_friend_request_modal, render_identity_modal};
pub use sidebar::render_sidebar;
pub use state::{AppState, AppAction};
pub use error::format_error_for_user;

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
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => clipboard.set_text(text).is_ok(),
        Err(_) => false,
    }
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
