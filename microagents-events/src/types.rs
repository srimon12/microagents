use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

const JSONRPC: &str = "2.0";

/// Result of a tool execution, either success with output or failure with an error message.
#[derive(Debug, Clone)]
pub enum ToolResult {
    /// Tool executed successfully.
    Ok(String),
    /// Tool execution failed.
    Err(String),
}

impl From<ToolResult> for Value {
    fn from(value: ToolResult) -> Self {
        match value {
            ToolResult::Ok(result) => Value::from(json!({
                "success": true,
                "result": result,
                "error": Value::Null,
            })),
            ToolResult::Err(error) => Value::from(json!({
                "success": false,
                "result": Value::Null,
                "error": error,
            })),
        }
    }
}

/// A JSON-RPC 2.0 notification message.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Map<String, Value>,
}

impl JsonRpcNotification {
    pub fn builder() -> Self {
        Self {
            jsonrpc: JSONRPC.into(),
            method: String::new(),
            params: Map::new(),
        }
    }

    pub fn method(mut self, method: String) -> Self {
        self.method = method;
        self
    }

    pub fn add_param(mut self, key: String, value: Value) -> Self {
        self.params.insert(key, value);
        self
    }
}

/// Trait for events that can be converted to JSON-RPC notifications and carry a session ID.
pub trait AgentEvent: Debug + Send + Sync {
    /// Convert this event into a [`JsonRpcNotification`].
    fn to_jsonrpc(self) -> JsonRpcNotification;
    /// Return the session ID associated with this event.
    fn session_id(self) -> String;
}
