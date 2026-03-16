//! MCP (Model Context Protocol) server — JSON-RPC 2.0 over stdio.
//!
//! Protocol: https://modelcontextprotocol.io/
//! Transport: stdin/stdout (line-delimited JSON)

use lokb_tools::{self, ToolContext};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Run MCP server on stdin/stdout.
pub fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = ToolContext::open().map_err(|e| format!("failed to open context: {e}"))?;

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    eprintln!("lokb MCP server started (stdio transport)");

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: serde_json::Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                    }),
                };
                writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                stdout.flush()?;
                continue;
            }
        };

        let id = request.id.unwrap_or(serde_json::Value::Null);
        let response = handle_request(&ctx, &request.method, request.params);

        let resp = match response {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(result),
                error: None,
            },
            Err(msg) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32000,
                    message: msg,
                }),
            },
        };

        writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
        stdout.flush()?;
    }

    Ok(())
}

fn handle_request(
    ctx: &ToolContext,
    method: &str,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    match method {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "lokb",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),

        "notifications/initialized" => Ok(serde_json::json!({})),

        "tools/list" => {
            let tools = lokb_tools::available_tools();
            Ok(serde_json::json!({ "tools": tools }))
        }

        "tools/call" => {
            let params = params.ok_or("missing params")?;
            let tool_name = params["name"].as_str().ok_or("missing tool name")?;
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            let result = call_tool(ctx, tool_name, arguments)?;

            Ok(serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
                }]
            }))
        }

        _ => Err(format!("unknown method: {method}")),
    }
}

fn call_tool(
    ctx: &ToolContext,
    name: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match name {
        "search" => {
            let input: lokb_tools::SearchInput =
                serde_json::from_value(arguments).map_err(|e| e.to_string())?;
            let output = lokb_tools::tool_search(ctx, &input)?;
            serde_json::to_value(output).map_err(|e| e.to_string())
        }
        "read" => {
            let input: lokb_tools::ReadInput =
                serde_json::from_value(arguments).map_err(|e| e.to_string())?;
            let output = lokb_tools::tool_read(ctx, &input)?;
            serde_json::to_value(output).map_err(|e| e.to_string())
        }
        "entity" => {
            let input: lokb_tools::EntityInput =
                serde_json::from_value(arguments).map_err(|e| e.to_string())?;
            let output = lokb_tools::tool_entity(ctx, &input)?;
            serde_json::to_value(output).map_err(|e| e.to_string())
        }
        "sources" => {
            let output = lokb_tools::tool_sources(ctx)?;
            serde_json::to_value(output).map_err(|e| e.to_string())
        }
        "status" => {
            let output = lokb_tools::tool_status(ctx)?;
            serde_json::to_value(output).map_err(|e| e.to_string())
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}
