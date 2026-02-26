//! Integration tests for daemon PID lifecycle, RPC serialization,
//! CLI argument parsing, and MCP tool mapping.

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Step 1: PID file lifecycle tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pid_file_lifecycle() {
    let temp = TempDir::new().unwrap();
    let pid_path = temp.path().join("chattor.pid");

    // Acquire PID file
    chattor::daemon::pid::acquire(&pid_path).unwrap();
    assert!(pid_path.exists());

    // Read PID
    let contents = std::fs::read_to_string(&pid_path).unwrap();
    assert_eq!(contents, format!("{}", std::process::id()));

    // Release
    chattor::daemon::pid::release(&pid_path);
    assert!(!pid_path.exists());
}

#[tokio::test]
async fn test_pid_file_prevents_double_start() {
    let temp = TempDir::new().unwrap();
    let pid_path = temp.path().join("chattor.pid");

    // Write a PID file with our own PID (simulates running process)
    std::fs::write(&pid_path, format!("{}", std::process::id())).unwrap();

    // Acquiring should fail
    let result = chattor::daemon::pid::acquire(&pid_path);
    assert!(result.is_err());

    // Cleanup
    std::fs::remove_file(&pid_path).ok();
}

#[tokio::test]
async fn test_pid_file_stale_cleanup() {
    let temp = TempDir::new().unwrap();
    let pid_path = temp.path().join("chattor.pid");

    // Write a stale PID (process 99999999 very likely doesn't exist)
    std::fs::write(&pid_path, "999999999").unwrap();

    // Acquiring should succeed (stale file cleaned up)
    chattor::daemon::pid::acquire(&pid_path).unwrap();

    // PID file should now contain our PID
    let contents = std::fs::read_to_string(&pid_path).unwrap();
    assert_eq!(contents, format!("{}", std::process::id()));

    chattor::daemon::pid::release(&pid_path);
}

// ---------------------------------------------------------------------------
// Step 2: RPC round-trip tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rpc_request_response_parsing() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"status","params":{}}"#;
    let req: chattor::daemon::rpc::RpcRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.method, "status");
}

#[test]
fn test_rpc_response_serialization() {
    let resp = chattor::daemon::rpc::RpcResponse::success(
        Some(serde_json::Value::Number(1.into())),
        serde_json::json!({"status": "ok"}),
    );
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"result\""));
    assert!(!json.contains("\"error\""));

    let resp = chattor::daemon::rpc::RpcResponse::error(
        Some(serde_json::Value::Number(1.into())),
        -32601,
        "Not found".into(),
    );
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"error\""));
    assert!(!json.contains("\"result\""));
}

#[test]
fn test_rpc_request_without_params() {
    let json = r#"{"jsonrpc":"2.0","id":null,"method":"identity"}"#;
    let req: chattor::daemon::rpc::RpcRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.method, "identity");
    assert_eq!(req.params, serde_json::Value::Null);
}

#[test]
fn test_rpc_response_null_id() {
    let resp = chattor::daemon::rpc::RpcResponse::success(None, serde_json::json!({"ok": true}));
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"id\":null"));
}

#[test]
fn test_rpc_error_codes() {
    let resp = chattor::daemon::rpc::RpcResponse::error(
        Some(serde_json::Value::Number(1.into())),
        -32700,
        "Parse error".into(),
    );
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("-32700"));
    assert!(json.contains("Parse error"));
}

// ---------------------------------------------------------------------------
// Step 3: CLI argument parsing tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_send_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "send", "abc.onion", "hello"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Send { peer, message }) => {
            assert_eq!(peer, "abc.onion");
            assert_eq!(message, "hello");
        }
        _ => panic!("Expected Send command"),
    }
}

#[test]
fn test_cli_daemon_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "daemon"]).unwrap();
    assert!(matches!(args.command, Some(chattor::cli::Command::Daemon)));
}

#[test]
fn test_cli_no_subcommand_defaults_to_tui() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor"]).unwrap();
    assert!(args.command.is_none()); // None = TUI
}

#[test]
fn test_cli_friends_list_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "friends", "list"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Friends {
            action: chattor::cli::FriendsAction::List,
        }) => {}
        _ => panic!("Expected Friends List"),
    }
}

#[test]
fn test_cli_mcp_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "mcp"]).unwrap();
    assert!(matches!(args.command, Some(chattor::cli::Command::Mcp)));
}

#[test]
fn test_cli_channels_publish_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from([
        "chattor",
        "channels",
        "publish",
        "public",
        "hello world",
    ])
    .unwrap();
    match args.command {
        Some(chattor::cli::Command::Channels {
            action:
                chattor::cli::ChannelsAction::Publish {
                    channel_type,
                    message,
                },
        }) => {
            assert_eq!(channel_type, "public");
            assert_eq!(message, "hello world");
        }
        _ => panic!("Expected Channels Publish"),
    }
}

#[test]
fn test_cli_status_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "status"]).unwrap();
    assert!(matches!(
        args.command,
        Some(chattor::cli::Command::Status)
    ));
}

#[test]
fn test_cli_identity_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "identity"]).unwrap();
    assert!(matches!(
        args.command,
        Some(chattor::cli::Command::Identity)
    ));
}

#[test]
fn test_cli_listen_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "listen"]).unwrap();
    assert!(matches!(
        args.command,
        Some(chattor::cli::Command::Listen)
    ));
}

#[test]
fn test_cli_recv_with_peer_filter() {
    use clap::Parser;
    let args =
        chattor::cli::Cli::try_parse_from(["chattor", "recv", "--peer", "abc.onion"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Recv { peer }) => {
            assert_eq!(peer.as_deref(), Some("abc.onion"));
        }
        _ => panic!("Expected Recv command"),
    }
}

#[test]
fn test_cli_recv_without_peer_filter() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "recv"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Recv { peer }) => {
            assert!(peer.is_none());
        }
        _ => panic!("Expected Recv command"),
    }
}

#[test]
fn test_cli_ephemeral_parses() {
    use clap::Parser;
    let args =
        chattor::cli::Cli::try_parse_from(["chattor", "ephemeral", "peer.onion", "3600"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Ephemeral { peer, ttl }) => {
            assert_eq!(peer, "peer.onion");
            assert_eq!(ttl, 3600);
        }
        _ => panic!("Expected Ephemeral command"),
    }
}

#[test]
fn test_cli_notifications_parses() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "notifications", "on"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Notifications { state }) => {
            assert_eq!(state, "on");
        }
        _ => panic!("Expected Notifications command"),
    }
}

#[test]
fn test_cli_friends_add_parses() {
    use clap::Parser;
    let args =
        chattor::cli::Cli::try_parse_from(["chattor", "friends", "add", "some-code"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Friends {
            action: chattor::cli::FriendsAction::Add { code },
        }) => {
            assert_eq!(code, "some-code");
        }
        _ => panic!("Expected Friends Add"),
    }
}

#[test]
fn test_cli_friends_accept_parses() {
    use clap::Parser;
    let args =
        chattor::cli::Cli::try_parse_from(["chattor", "friends", "accept", "42"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Friends {
            action: chattor::cli::FriendsAction::Accept { id },
        }) => {
            assert_eq!(id, 42);
        }
        _ => panic!("Expected Friends Accept"),
    }
}

#[test]
fn test_cli_friends_reject_parses() {
    use clap::Parser;
    let args =
        chattor::cli::Cli::try_parse_from(["chattor", "friends", "reject", "7"]).unwrap();
    match args.command {
        Some(chattor::cli::Command::Friends {
            action: chattor::cli::FriendsAction::Reject { id },
        }) => {
            assert_eq!(id, 7);
        }
        _ => panic!("Expected Friends Reject"),
    }
}

#[test]
fn test_cli_debug_flag() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from(["chattor", "--debug"]).unwrap();
    assert!(args.debug);
}

#[test]
fn test_cli_config_dir_override() {
    use clap::Parser;
    let args = chattor::cli::Cli::try_parse_from([
        "chattor",
        "--config-dir",
        "/tmp/chattor-cfg",
    ])
    .unwrap();
    assert_eq!(args.config_dir.as_deref(), Some("/tmp/chattor-cfg"));
}

#[test]
fn test_cli_data_dir_override() {
    use clap::Parser;
    let args =
        chattor::cli::Cli::try_parse_from(["chattor", "--data-dir", "/tmp/chattor-data"]).unwrap();
    assert_eq!(args.data_dir.as_deref(), Some("/tmp/chattor-data"));
}

// ---------------------------------------------------------------------------
// Step 4: MCP tool mapping tests
// ---------------------------------------------------------------------------

#[test]
fn test_mcp_tool_definitions() {
    let tools = chattor::mcp::tools::tool_definitions();
    assert_eq!(tools.len(), 9);

    // Check all tools have required fields
    for tool in &tools {
        assert!(tool.get("name").is_some());
        assert!(tool.get("description").is_some());
        assert!(tool.get("inputSchema").is_some());
    }
}

#[test]
fn test_mcp_tool_names() {
    let tools = chattor::mcp::tools::tool_definitions();
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();

    assert!(names.contains(&"send_message"));
    assert!(names.contains(&"receive_messages"));
    assert!(names.contains(&"list_friends"));
    assert!(names.contains(&"add_friend"));
    assert!(names.contains(&"accept_friend_request"));
    assert!(names.contains(&"get_identity"));
    assert!(names.contains(&"get_status"));
    assert!(names.contains(&"publish_channel_post"));
    assert!(names.contains(&"list_channel_posts"));
}

#[test]
fn test_mcp_tool_to_rpc_mapping() {
    use chattor::mcp::tools::tool_to_rpc;

    assert_eq!(
        tool_to_rpc("send_message", &serde_json::json!({}))
            .unwrap()
            .0,
        "send_message"
    );
    assert_eq!(
        tool_to_rpc("list_friends", &serde_json::json!({}))
            .unwrap()
            .0,
        "friends_list"
    );
    assert_eq!(
        tool_to_rpc("get_status", &serde_json::json!({}))
            .unwrap()
            .0,
        "status"
    );
    assert_eq!(
        tool_to_rpc("receive_messages", &serde_json::json!({}))
            .unwrap()
            .0,
        "recv_messages"
    );
    assert_eq!(
        tool_to_rpc("add_friend", &serde_json::json!({}))
            .unwrap()
            .0,
        "friends_add"
    );
    assert_eq!(
        tool_to_rpc("accept_friend_request", &serde_json::json!({}))
            .unwrap()
            .0,
        "friends_accept"
    );
    assert_eq!(
        tool_to_rpc("get_identity", &serde_json::json!({}))
            .unwrap()
            .0,
        "identity"
    );
    assert_eq!(
        tool_to_rpc("publish_channel_post", &serde_json::json!({}))
            .unwrap()
            .0,
        "channels_publish"
    );
    assert_eq!(
        tool_to_rpc("list_channel_posts", &serde_json::json!({}))
            .unwrap()
            .0,
        "channels_feed"
    );
    assert!(tool_to_rpc("nonexistent", &serde_json::json!({})).is_none());
}

#[test]
fn test_mcp_tool_to_rpc_passes_arguments() {
    use chattor::mcp::tools::tool_to_rpc;

    let args = serde_json::json!({"peer": "abc.onion", "message": "hello"});
    let (method, params) = tool_to_rpc("send_message", &args).unwrap();
    assert_eq!(method, "send_message");
    assert_eq!(params["peer"], "abc.onion");
    assert_eq!(params["message"], "hello");
}

#[test]
fn test_mcp_tool_to_rpc_list_friends_ignores_args() {
    use chattor::mcp::tools::tool_to_rpc;

    // list_friends always passes empty params regardless of input
    let args = serde_json::json!({"extra": "ignored"});
    let (method, params) = tool_to_rpc("list_friends", &args).unwrap();
    assert_eq!(method, "friends_list");
    assert_eq!(params, serde_json::json!({}));
}
