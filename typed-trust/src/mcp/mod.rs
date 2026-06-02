//! Phase 3 MCP server module.
//!
//! Hand-rolled JSON-RPC 2.0 over stdio. The MCP protocol surface
//! this server implements is small enough that introducing an
//! abstraction layer would cost more than it saves: three method
//! names (`initialize`, `tools/list`, `tools/call`), one
//! line-delimited JSON-per-line framing, and a server-info
//! handshake.
//!
//! Error tier discipline (codex F-3-10):
//! - **Tier 1** (protocol error): the response carries an `error`
//!   object. Reserved for invalid input shape, server
//!   misconfiguration (no `--allow-manifest`), and unauthorized
//!   paths where the request itself is malformed.
//! - **Tier 2** (tool result with `isError: true`): the response
//!   carries a `result` whose `isError` flag is set. Reserved for
//!   corpus data errors — malformed sidecars, unknown claim ids,
//!   bad includes, unsupported render formats, missing files.
//!   Claude can recover from these.
//! - **Process failure**: never on corpus data. Tool handlers
//!   return `Result<T, ToolError>`; `spawn_blocking` panics are
//!   converted to tier-1 errors so the server stays alive.

pub mod handlers;
pub mod tools;

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::loader::AllowListPathPolicy;

pub use handlers::{ServerState, ToolError, ToolErrorTier};

/// MCP protocol version this server speaks.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Run the MCP server: read JSON-RPC frames from stdin, dispatch
/// to handlers, write responses to stdout. Each request handed off
/// to a `spawn_blocking` worker so typed-trust sync code stays
/// sync.
pub async fn run(state: Arc<ServerState>) -> std::io::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    // Track spawned dispatch tasks so we drain in-flight work
    // when stdin closes — avoids the tokio "JoinHandle polled after
    // completion" race that surfaces when the runtime is dropped
    // while tasks are still resolving their final write.
    let mut in_flight = tokio::task::JoinSet::new();

    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": "Parse error"}
                });
                write_frame(&stdout, resp).await?;
                continue;
            }
        };

        let state = state.clone();
        let stdout = stdout.clone();
        in_flight.spawn(async move {
            if let Some(resp) = dispatch(state, req).await {
                let _ = write_frame(&stdout, resp).await;
            }
        });
    }
    // Drain in-flight tasks before returning so the runtime
    // doesn't tear down their futures mid-write.
    while in_flight.join_next().await.is_some() {}
    Ok(())
}

async fn write_frame(
    stdout: &Arc<Mutex<tokio::io::Stdout>>,
    value: Value,
) -> std::io::Result<()> {
    let serialized = serde_json::to_string(&value).expect("serialize JSON-RPC frame");
    let mut guard = stdout.lock().await;
    guard.write_all(serialized.as_bytes()).await?;
    guard.write_all(b"\n").await?;
    guard.flush().await?;
    Ok(())
}

async fn dispatch(state: Arc<ServerState>, req: Value) -> Option<Value> {
    let id = req.get("id").cloned()?;
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "typed-trust-mcp", "version": env!("CARGO_PKG_VERSION")}
            }
        })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"tools": tools::tool_definitions()}
        })),
        "tools/call" => Some(handle_tool_call(state, id, params).await),
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("Method not found: {method}")}
        })),
    }
}

async fn handle_tool_call(state: Arc<ServerState>, id: Value, params: Value) -> Value {
    let tool_name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32602, "message": "missing tool name"}
            });
        }
    };
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

    let state_clone = state.clone();
    let tool_name_clone = tool_name.clone();
    let join_result = tokio::task::spawn_blocking(move || {
        handlers::dispatch_sync(&state_clone, &tool_name_clone, arguments)
    })
    .await;

    match join_result {
        Ok(Ok(result_value)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{"type": "text", "text": serde_json::to_string(&result_value)
                    .unwrap_or_else(|_| String::from("{}"))}],
                "isError": false
            }
        }),
        Ok(Err(err)) => match err.tier {
            ToolErrorTier::Protocol => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": err.code, "message": err.message}
            }),
            ToolErrorTier::Data => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{"type": "text", "text": format!("error: {}", err.message)}],
                    "isError": true
                }
            }),
        },
        Err(join_err) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32603, "message": format!("internal: handler panicked ({join_err})")}
        }),
    }
}

/// Construct server state from an allow-list policy.
pub fn build_state(policy: AllowListPathPolicy) -> Arc<ServerState> {
    Arc::new(ServerState::new(policy))
}
