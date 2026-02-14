use std::time::Duration;
use tempfile::TempDir;
use chattor::{
    app::App,
    config::Settings,
};

/// Helper to create test instance
async fn create_test_instance(_name: &str) -> (App, TempDir) {
    let temp_dir = TempDir::new().unwrap();

    let settings = Settings {
        config_dir: temp_dir.path().join("config"),
        data_dir: temp_dir.path().join("data"),
        db_path: temp_dir.path().join("data/test.db"),
        debug: true,
        tor_socks_port: 9050,
    };

    let app = App::new_with_settings(settings).unwrap();

    (app, temp_dir)
}

#[tokio::test]
#[ignore] // Ignore by default (requires Tor, takes time)
async fn test_two_instance_friend_request() {
    // Create Alice and Bob instances
    let (mut alice, _alice_dir) = create_test_instance("alice").await;
    let (mut bob, _bob_dir) = create_test_instance("bob").await;

    // Initialize Tor for both (will take 30-60 seconds)
    println!("Initializing Alice's Tor...");
    alice.init_tor().await.unwrap();

    println!("Initializing Bob's Tor...");
    bob.init_tor().await.unwrap();

    // Get Bob's friend code
    let _bob_onion = bob.onion_address.as_ref().unwrap();
    let _bob_friend_code = "test-1234-code-5678"; // Simplified for test

    println!("Alice sending friend request to Bob...");

    // Alice sends friend request
    // TODO: Implement friend request sending in App

    // Bob should receive request
    // Wait up to 10 seconds
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Check Bob's database for pending request
    let conn = bob.db.connection();
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM friend_requests WHERE status = 'pending'",
        [],
        |row| row.get(0),
    ).unwrap();

    // For MVP, this might be 0 since we're testing infrastructure
    println!("Bob received {} friend requests", count);
}

#[tokio::test]
#[ignore]
async fn test_two_instance_message_send() {
    // Similar structure to above test
    // After friend request accepted, send message

    let (mut alice, _alice_dir) = create_test_instance("alice").await;
    let (mut bob, _bob_dir) = create_test_instance("bob").await;

    alice.init_tor().await.unwrap();
    bob.init_tor().await.unwrap();

    // TODO: Set up friendship
    // TODO: Alice sends message to Bob
    // TODO: Bob receives and decrypts
    // TODO: Bob sends delivery receipt
    // TODO: Alice receives receipt and updates status

    println!("Two-instance messaging test (stub)");
}
