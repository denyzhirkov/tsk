//! MCP (Model Context Protocol) server implementation
//!
//! Provides JSON-RPC 2.0 interface over stdio for IDE integration.

use crate::{
    complete_task, create_memory, create_task, get_memory, get_task, init_project, list_memories,
    list_tasks, open_db, remove_memory, remove_task, search_memories, start_task, update_task,
};
use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "tsk";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

const NOT_INITIALIZED_ERROR: &str =
    "Project not initialized. Run 'tsk init' in terminal or use the 'init' tool.";

// ============================================================================
// JSON-RPC Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

// ============================================================================
// MCP Types
// ============================================================================

#[derive(Debug, Serialize)]
struct ServerInfo {
    name: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    capabilities: Capabilities,
    #[serde(rename = "serverInfo")]
    server_info: ServerInfo,
}

#[derive(Debug, Serialize)]
struct Capabilities {
    tools: ToolsCapability,
}

#[derive(Debug, Serialize)]
struct ToolsCapability {
    #[serde(rename = "listChanged")]
    list_changed: bool,
}

#[derive(Debug, Serialize)]
struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Debug, Serialize)]
struct ToolsListResult {
    tools: Vec<Tool>,
}

#[derive(Debug, Serialize)]
struct ToolResult {
    content: Vec<ToolContent>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ToolContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

impl ToolResult {
    fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: text.into(),
            }],
            is_error: None,
        }
    }

    fn json<T: Serialize>(value: &T) -> Self {
        Self::text(serde_json::to_string_pretty(value).unwrap_or_default())
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: message.into(),
            }],
            is_error: Some(true),
        }
    }
}

// ============================================================================
// Tool Definitions
// ============================================================================

fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "init".to_string(),
            description: "Initialize tsk in current directory. Creates .tsk/ folder with database."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "create".to_string(),
            description: "Create a new task".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Task title (short summary)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Task description (detailed info)"
                    },
                    "parent": {
                        "type": "string",
                        "description": "Parent task ID for subtasks"
                    },
                    "depend": {
                        "type": "string",
                        "description": "Dependency task ID"
                    }
                },
                "required": ["title", "description"]
            }),
        },
        Tool {
            name: "list".to_string(),
            description: "List tasks. By default shows pending tasks only.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "inprogress": {
                        "type": "boolean",
                        "description": "Show in progress tasks only"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Show all tasks (pending, in progress, done)"
                    },
                    "parent": {
                        "type": "string",
                        "description": "Filter by parent task ID"
                    }
                }
            }),
        },
        Tool {
            name: "show".to_string(),
            description: "Show full task details".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Task ID (6 characters)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "update".to_string(),
            description: "Update task description".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Task ID (6 characters)"
                    },
                    "description": {
                        "type": "string",
                        "description": "New description text"
                    }
                },
                "required": ["id", "description"]
            }),
        },
        Tool {
            name: "start".to_string(),
            description: "Start working on a task (pending -> in progress)".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Task ID (6 characters)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "done".to_string(),
            description: "Mark task as done".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Task ID (6 characters)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "remove".to_string(),
            description: "Remove a task".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Task ID (6 characters)"
                    }
                },
                "required": ["id"]
            }),
        },
        // Memory tools
        Tool {
            name: "memory_create".to_string(),
            description: "Create a memory entry to store project knowledge".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Memory content text"
                    },
                    "tags": {
                        "type": "string",
                        "description": "Tags (comma-separated)"
                    }
                },
                "required": ["content"]
            }),
        },
        Tool {
            name: "memory_list".to_string(),
            description: "List memory entries".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tag": {
                        "type": "string",
                        "description": "Filter by tag"
                    },
                    "last": {
                        "type": "integer",
                        "description": "Show only last N entries"
                    }
                }
            }),
        },
        Tool {
            name: "memory_show".to_string(),
            description: "Show full memory entry".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Memory ID (6 characters)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "memory_search".to_string(),
            description: "Search memories by content".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "memory_remove".to_string(),
            description: "Remove a memory entry".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Memory ID (6 characters)"
                    }
                },
                "required": ["id"]
            }),
        },
    ]
}

// ============================================================================
// Tool Handlers
// ============================================================================

fn handle_tool_call(name: &str, args: &Value) -> ToolResult {
    match name {
        "init" => handle_init(),
        _ => {
            // All other tools require initialized project
            match open_db() {
                Ok(Some(conn)) => handle_tool_with_db(&conn, name, args),
                Ok(None) => ToolResult::error(NOT_INITIALIZED_ERROR),
                Err(e) => ToolResult::error(format!("Database error: {}", e)),
            }
        }
    }
}

fn handle_tool_with_db(conn: &Connection, name: &str, args: &Value) -> ToolResult {
    match name {
        "create" => handle_create(conn, args),
        "list" => handle_list(conn, args),
        "show" => handle_show(conn, args),
        "update" => handle_update(conn, args),
        "start" => handle_start(conn, args),
        "done" => handle_done(conn, args),
        "remove" => handle_remove(conn, args),
        // Memory tools
        "memory_create" => handle_memory_create(conn, args),
        "memory_list" => handle_memory_list(conn, args),
        "memory_show" => handle_memory_show(conn, args),
        "memory_search" => handle_memory_search(conn, args),
        "memory_remove" => handle_memory_remove(conn, args),
        _ => ToolResult::error(format!("Unknown tool: {}", name)),
    }
}

fn handle_init() -> ToolResult {
    match init_project() {
        Ok(path) => ToolResult::json(&json!({
            "success": true,
            "path": path.to_string_lossy()
        })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_create(conn: &Connection, args: &Value) -> ToolResult {
    let title = args["title"].as_str().unwrap_or_default();
    let description = args["description"].as_str().unwrap_or_default();
    let parent = args["parent"].as_str();
    let depend = args["depend"].as_str();

    match create_task(conn, title, description, parent, depend) {
        Ok(id) => ToolResult::json(&json!({ "id": id })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_list(conn: &Connection, args: &Value) -> ToolResult {
    let inprogress = args["inprogress"].as_bool().unwrap_or(false);
    let all = args["all"].as_bool().unwrap_or(false);
    let parent = args["parent"].as_str();

    match list_tasks(conn, inprogress, all, parent) {
        Ok(tasks) => ToolResult::json(&tasks),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_show(conn: &Connection, args: &Value) -> ToolResult {
    let id = match args["id"].as_str() {
        Some(id) => id,
        None => return ToolResult::error("Missing required parameter: id"),
    };

    match get_task(conn, id) {
        Ok(task) => ToolResult::json(&task),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_update(conn: &Connection, args: &Value) -> ToolResult {
    let id = match args["id"].as_str() {
        Some(id) => id,
        None => return ToolResult::error("Missing required parameter: id"),
    };
    let description = args["description"].as_str().unwrap_or_default();

    match update_task(conn, id, description) {
        Ok(()) => ToolResult::json(&json!({ "success": true, "id": id })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_start(conn: &Connection, args: &Value) -> ToolResult {
    let id = match args["id"].as_str() {
        Some(id) => id,
        None => return ToolResult::error("Missing required parameter: id"),
    };

    match start_task(conn, id) {
        Ok(()) => ToolResult::json(&json!({ "success": true, "id": id })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_done(conn: &Connection, args: &Value) -> ToolResult {
    let id = match args["id"].as_str() {
        Some(id) => id,
        None => return ToolResult::error("Missing required parameter: id"),
    };

    match complete_task(conn, id) {
        Ok(()) => ToolResult::json(&json!({ "success": true, "id": id })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_remove(conn: &Connection, args: &Value) -> ToolResult {
    let id = match args["id"].as_str() {
        Some(id) => id,
        None => return ToolResult::error("Missing required parameter: id"),
    };

    match remove_task(conn, id) {
        Ok(()) => ToolResult::json(&json!({ "success": true, "id": id })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

// ============================================================================
// Memory Handlers
// ============================================================================

fn handle_memory_create(conn: &Connection, args: &Value) -> ToolResult {
    let content = args["content"].as_str().unwrap_or_default();
    let tags = args["tags"].as_str();

    match create_memory(conn, content, tags) {
        Ok(id) => ToolResult::json(&json!({ "id": id })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_memory_list(conn: &Connection, args: &Value) -> ToolResult {
    let tag = args["tag"].as_str();
    let last = args["last"].as_u64().map(|n| n as usize);

    match list_memories(conn, tag, last) {
        Ok(memories) => ToolResult::json(&memories),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_memory_show(conn: &Connection, args: &Value) -> ToolResult {
    let id = match args["id"].as_str() {
        Some(id) => id,
        None => return ToolResult::error("Missing required parameter: id"),
    };

    match get_memory(conn, id) {
        Ok(memory) => ToolResult::json(&memory),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_memory_search(conn: &Connection, args: &Value) -> ToolResult {
    let query = match args["query"].as_str() {
        Some(q) => q,
        None => return ToolResult::error("Missing required parameter: query"),
    };

    match search_memories(conn, query) {
        Ok(memories) => ToolResult::json(&memories),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

fn handle_memory_remove(conn: &Connection, args: &Value) -> ToolResult {
    let id = match args["id"].as_str() {
        Some(id) => id,
        None => return ToolResult::error("Missing required parameter: id"),
    };

    match remove_memory(conn, id) {
        Ok(()) => ToolResult::json(&json!({ "success": true, "id": id })),
        Err(e) => ToolResult::error(e.to_string()),
    }
}

// ============================================================================
// Request Handler
// ============================================================================

fn handle_request(request: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = request.id.clone();

    match request.method.as_str() {
        "initialize" => Some(JsonRpcResponse::success(
            id,
            serde_json::to_value(InitializeResult {
                protocol_version: PROTOCOL_VERSION.to_string(),
                capabilities: Capabilities {
                    tools: ToolsCapability { list_changed: false },
                },
                server_info: ServerInfo {
                    name: SERVER_NAME.to_string(),
                    version: SERVER_VERSION.to_string(),
                },
            })
            .unwrap(),
        )),

        "notifications/initialized" => None, // No response for notifications

        "tools/list" => Some(JsonRpcResponse::success(
            id,
            serde_json::to_value(ToolsListResult { tools: get_tools() }).unwrap(),
        )),

        "tools/call" => {
            let name = request.params["name"].as_str().unwrap_or_default();
            let args = &request.params["arguments"];
            let result = handle_tool_call(name, args);
            Some(JsonRpcResponse::success(
                id,
                serde_json::to_value(result).unwrap(),
            ))
        }

        _ => Some(JsonRpcResponse::error(
            id,
            -32601,
            format!("Method not found: {}", request.method),
        )),
    }
}

// ============================================================================
// Server Main Loop
// ============================================================================

pub fn run_server() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
                stdout.flush()?;
                continue;
            }
        };

        if let Some(response) = handle_request(request) {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }

    Ok(())
}
