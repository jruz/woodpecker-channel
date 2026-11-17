pub const INVALID_PARAMS: i64 = -32602;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const PARSE_ERROR: i64 = -32700;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid JSON-RPC message")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawMessage {
    id: Option<serde_json::Value>,
    method: Option<String>,
    params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Notification {
        method: String,
    },
    Request {
        id: serde_json::Value,
        method: String,
        params: Option<serde_json::Value>,
    },
    Response,
}

fn classify(raw: RawMessage) -> Incoming {
    match (raw.id, raw.method) {
        (Some(id), Some(method)) => Incoming::Request {
            id,
            method,
            params: raw.params,
        },
        (None, Some(method)) => Incoming::Notification { method },
        (_, None) => Incoming::Response,
    }
}

pub fn parse(line: &str) -> Result<Incoming, Error> {
    let raw: RawMessage = serde_json::from_str(line)?;
    let incoming = classify(raw);
    Ok(incoming)
}

#[must_use]
pub fn result(id: &serde_json::Value, result: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result})
}

#[must_use]
pub fn error(id: &serde_json::Value, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    })
}

#[must_use]
pub fn notification(method: &str, params: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params})
}

#[must_use]
pub fn tool_text(text: &str, is_error: bool) -> serde_json::Value {
    serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "isError": is_error,
    })
}
