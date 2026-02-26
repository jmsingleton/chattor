use crate::error::{ChattorError, Result};
use serde_json::Value;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Send an RPC request to the daemon and return the result.
pub async fn rpc_call(data_dir: &Path, method: &str, params: Value) -> Result<Value> {
    let socket_path = data_dir.join("chattor.sock");

    let stream = UnixStream::connect(&socket_path).await.map_err(|_| {
        ChattorError::Network(
            "Cannot connect to daemon. Is it running? Start with: chattor daemon".into(),
        )
    })?;

    let (reader, mut writer) = stream.into_split();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let line = format!("{}\n", serde_json::to_string(&request).unwrap());
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| ChattorError::Network(format!("Socket write error: {}", e)))?;

    let mut lines = BufReader::new(reader).lines();
    let response_line = lines
        .next_line()
        .await
        .map_err(|e| ChattorError::Network(format!("Socket read error: {}", e)))?
        .ok_or_else(|| ChattorError::Network("Daemon closed connection".into()))?;

    let response: Value = serde_json::from_str(&response_line)
        .map_err(|e| ChattorError::Network(format!("Invalid response: {}", e)))?;

    if let Some(error) = response.get("error") {
        let msg = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error");
        return Err(ChattorError::Network(msg.to_string()));
    }

    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}
