#[path = "../support/mod.rs"]
mod transport;

use serde_json::{Value, json};
pub use transport::McpClient;

pub fn call(client: &mut McpClient, name: &str, arguments: Value) -> Value {
    client.send_request(json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    }))["result"]
        .clone()
}

pub fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap()
}

pub fn success(client: &mut McpClient, name: &str, arguments: Value) -> Value {
    let result = call(client, name, arguments);
    assert_ne!(result["isError"], true, "{name}: {}", text(&result));
    serde_json::from_str(text(&result)).unwrap()
}

pub fn failure(client: &mut McpClient, name: &str, arguments: Value) -> String {
    let result = call(client, name, arguments);
    assert_eq!(result["isError"], true, "{name}: {}", text(&result));
    text(&result).to_string()
}
