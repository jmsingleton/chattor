use super::tools;
use crate::client;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub async fn run(data_dir: PathBuf) -> crate::error::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let response = match method {
            "initialize" => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "chattor",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                })
            }
            "notifications/initialized" => continue, // No response needed
            "tools/list" => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": tools::tool_definitions()
                    }
                })
            }
            "tools/call" => {
                let tool_name = request["params"]["name"].as_str().unwrap_or("");
                let arguments = request["params"]
                    .get("arguments")
                    .cloned()
                    .unwrap_or(json!({}));

                match tools::tool_to_rpc(tool_name, &arguments) {
                    Some((rpc_method, rpc_params)) => {
                        match client::rpc_call(&data_dir, rpc_method, rpc_params).await {
                            Ok(result) => {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [{
                                            "type": "text",
                                            "text": serde_json::to_string_pretty(&result).unwrap_or_default()
                                        }]
                                    }
                                })
                            }
                            Err(e) => {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [{
                                            "type": "text",
                                            "text": format!("Error: {}", e)
                                        }],
                                        "isError": true
                                    }
                                })
                            }
                        }
                    }
                    None => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32601,
                                "message": format!("Unknown tool: {}", tool_name)
                            }
                        })
                    }
                }
            }
            _ => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("Method not found: {}", method) }
                })
            }
        };

        let output = serde_json::to_string(&response).unwrap();
        stdout
            .write_all(format!("{}\n", output).as_bytes())
            .await
            .ok();
        stdout.flush().await.ok();
    }

    Ok(())
}
