use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

const JSONRPC: &str = "2.0";

/// Errors that can occur when parsing a [`JsonRpcNotification`] into an [`AgentEventAny`].
#[derive(Debug, Clone)]
pub enum AgentEventError {
    /// A required field was missing from the JSON-RPC params.
    MissingField(String),
    /// A field had an unexpected type.
    InvalidFieldType(String),
    /// The JSON-RPC method name is not recognized.
    UnknownMethod(String),
}

impl std::fmt::Display for AgentEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentEventError::MissingField(field) => write!(f, "Missing required field: {}", field),
            AgentEventError::InvalidFieldType(field) => {
                write!(f, "Invalid type for field: {}", field)
            }
            AgentEventError::UnknownMethod(method) => write!(f, "Unknown method: {}", method),
        }
    }
}

impl std::error::Error for AgentEventError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

impl From<ToolCall> for Value {
    fn from(value: ToolCall) -> Self {
        let tc = serde_json::to_string(&value).expect("Should be serializable");
        let v: Value = serde_json::from_str(&tc).expect("Should deserialize to a generic value");
        v
    }
}

impl TryFrom<Value> for ToolCall {
    type Error = AgentEventError;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let v = serde_json::to_string(&value).map_err(|e| {
            AgentEventError::InvalidFieldType(format!(
                "Value for a tool call should be serializable, got: {}",
                e.to_string()
            ))
        })?;
        let tc: ToolCall = serde_json::from_str(&v).map_err(|e| {
            AgentEventError::InvalidFieldType(format!(
                "Value for a tool call should deserialize in a ToolCall, got: {}",
                e.to_string()
            ))
        })?;
        Ok(tc)
    }
}

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
