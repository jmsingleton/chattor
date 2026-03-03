use serde_json::{json, Value};

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "send_message",
            "description": "Send a private encrypted message to a peer over Tor",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "peer": { "type": "string", "description": "Peer .onion address or friend code" },
                    "message": { "type": "string", "description": "Message text to send" }
                },
                "required": ["peer", "message"]
            }
        }),
        json!({
            "name": "receive_messages",
            "description": "Get unread messages, optionally filtered by peer",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "peer": { "type": "string", "description": "Optional: filter by peer onion/code" },
                    "since": { "type": "integer", "description": "Optional: Unix timestamp, return messages after this time" }
                }
            }
        }),
        json!({
            "name": "list_friends",
            "description": "List all friends with their online/typing status and unread counts",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "add_friend",
            "description": "Send a friend request to a peer via their .onion address or friend code",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": ".onion address or friend code" }
                },
                "required": ["code"]
            }
        }),
        json!({
            "name": "accept_friend_request",
            "description": "Accept a pending friend request by ID",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Friend request ID" }
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "get_identity",
            "description": "Get own friend code and .onion address for sharing with peers",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "get_status",
            "description": "Check daemon status, Tor connection, and .onion address",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "publish_channel_post",
            "description": "Publish a post to own broadcast channel",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channel_type": { "type": "string", "enum": ["public", "friends"], "description": "Channel type" },
                    "message": { "type": "string", "description": "Post content" }
                },
                "required": ["channel_type", "message"]
            }
        }),
        json!({
            "name": "list_channel_posts",
            "description": "Read posts from a broadcast channel",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channel_id": { "type": "integer", "description": "Optional: specific channel ID" }
                }
            }
        }),
    ]
}

/// Map MCP tool name to daemon RPC method + params.
pub fn tool_to_rpc(tool_name: &str, arguments: &Value) -> Option<(&'static str, Value)> {
    match tool_name {
        "send_message" => Some(("send_message", arguments.clone())),
        "receive_messages" => Some(("recv_messages", arguments.clone())),
        "list_friends" => Some(("friends_list", json!({}))),
        "add_friend" => Some(("friends_add", arguments.clone())),
        "accept_friend_request" => Some(("friends_accept", arguments.clone())),
        "get_identity" => Some(("identity", json!({}))),
        "get_status" => Some(("status", json!({}))),
        "publish_channel_post" => Some(("channels_publish", arguments.clone())),
        "list_channel_posts" => Some(("channels_feed", arguments.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_count() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 9); // 9 MCP tools
    }

    #[test]
    fn test_tool_definitions_have_required_fields() {
        for tool in tool_definitions() {
            assert!(tool.get("name").is_some());
            assert!(tool.get("description").is_some());
            assert!(tool.get("inputSchema").is_some());
        }
    }

    #[test]
    fn test_tool_to_rpc_mapping() {
        assert_eq!(
            tool_to_rpc("send_message", &json!({})).unwrap().0,
            "send_message"
        );
        assert_eq!(
            tool_to_rpc("receive_messages", &json!({})).unwrap().0,
            "recv_messages"
        );
        assert_eq!(
            tool_to_rpc("list_friends", &json!({})).unwrap().0,
            "friends_list"
        );
        assert_eq!(
            tool_to_rpc("add_friend", &json!({})).unwrap().0,
            "friends_add"
        );
        assert_eq!(
            tool_to_rpc("accept_friend_request", &json!({})).unwrap().0,
            "friends_accept"
        );
        assert_eq!(
            tool_to_rpc("get_identity", &json!({})).unwrap().0,
            "identity"
        );
        assert_eq!(tool_to_rpc("get_status", &json!({})).unwrap().0, "status");
        assert_eq!(
            tool_to_rpc("publish_channel_post", &json!({})).unwrap().0,
            "channels_publish"
        );
        assert_eq!(
            tool_to_rpc("list_channel_posts", &json!({})).unwrap().0,
            "channels_feed"
        );
    }

    #[test]
    fn test_tool_to_rpc_unknown() {
        assert!(tool_to_rpc("nonexistent_tool", &json!({})).is_none());
    }

    #[test]
    fn test_tool_to_rpc_passes_arguments() {
        let args = json!({"peer": "abc.onion", "message": "hello"});
        let (_, params) = tool_to_rpc("send_message", &args).unwrap();
        assert_eq!(params["peer"], "abc.onion");
        assert_eq!(params["message"], "hello");
    }
}
