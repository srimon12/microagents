pub mod types;

use serde_json::Value;

use std::convert::TryFrom;

use crate::types::{AgentEvent, JsonRpcNotification, ToolResult};

/// Indicates whether a session is being started fresh or resumed.
#[derive(Debug, Clone)]
pub enum SessionInitType {
    /// Start a new session.
    Start,
    /// Resume an existing session.
    Resume,
}

impl From<SessionInitType> for Value {
    fn from(value: SessionInitType) -> Self {
        match value {
            SessionInitType::Start => Value::from("start"),
            SessionInitType::Resume => Value::from("resume"),
        }
    }
}

/// Type of content delta in a stream.
#[derive(Debug, Clone)]
pub enum DeltaType {
    /// Regular text content.
    Text,
    /// Model thinking or reasoning content.
    Thinking,
}

impl From<DeltaType> for Value {
    fn from(value: DeltaType) -> Self {
        match value {
            DeltaType::Text => Value::from("text"),
            DeltaType::Thinking => Value::from("thinking"),
        }
    }
}

/// Event emitted when a session is initialized.
#[derive(Debug, Clone)]
pub struct SessionInitEvent {
    pub session_id: String,
    pub model: String,
    pub provider: String,
    pub system: String,
    /// Either `'new'` or `'resume'`.
    pub init_type: String,
}

/// Event emitted when a session stops.
#[derive(Debug, Clone)]
pub struct SessionStopEvent {
    pub session_id: String,
    pub success: bool,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// Event emitted when the user submits a prompt.
#[derive(Debug, Clone)]
pub struct UserPromptSubmitEvent {
    pub session_id: String,
    pub turn_id: String,
    pub prompt: String,
}

/// Event emitted for each delta in a streaming response.
#[derive(Debug, Clone)]
pub struct StreamDeltaEvent {
    pub session_id: String,
    pub turn_id: String,
    pub delta: String,
    pub delta_type: DeltaType,
}

/// Event emitted when a tool is called.
#[derive(Debug, Clone)]
pub struct ToolCallEvent {
    pub session_id: String,
    pub turn_id: String,
    pub name: String,
    pub input: Value,
}

/// Event emitted when a tool returns a result.
#[derive(Debug, Clone)]
pub struct ToolResultEvent {
    pub session_id: String,
    pub turn_id: String,
    /// Tool execution result. [`Value`] implements `From<ToolResult>`.
    pub result: ToolResult,
}

/// Event emitted when a skill is loaded.
#[derive(Debug, Clone)]
pub struct SkillLoadEvent {
    pub session_id: String,
    pub turn_id: String,
    pub skill_name: String,
}

/// Event emitted when the assistant produces a complete response.
#[derive(Debug, Clone)]
pub struct AssistantResponseEvent {
    pub session_id: String,
    pub turn_id: String,
    pub full_thinking: String,
    pub full_text: String,
}

impl AgentEvent for SessionInitEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("session.init".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("system".into(), Value::from(self.system))
            .add_param("model".into(), Value::from(self.model))
            .add_param("provider".into(), Value::from(self.provider))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

impl AgentEvent for SessionStopEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("session.stop".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("success".into(), Value::from(self.success))
            .add_param("result".into(), Value::from(self.result))
            .add_param("error".into(), Value::from(self.error))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

impl AgentEvent for UserPromptSubmitEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("user.prompt.submit".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("prompt".into(), Value::from(self.prompt))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

impl AgentEvent for StreamDeltaEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("stream.delta".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("delta".into(), Value::from(self.delta))
            .add_param("delta_type".into(), Value::from(self.delta_type))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

impl AgentEvent for ToolCallEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("tool.call".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("name".into(), Value::from(self.name))
            .add_param("input".into(), Value::from(self.input))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

impl AgentEvent for ToolResultEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("tool.result".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("result".into(), Value::from(self.result))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

impl AgentEvent for SkillLoadEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("skill.load".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("skill_name".into(), Value::from(self.skill_name))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

impl AgentEvent for AssistantResponseEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("assistant.response".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("full_thinking".into(), Value::from(self.full_thinking))
            .add_param("full_text".into(), Value::from(self.full_text))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

/// A sum type wrapping any agent event.
#[derive(Debug, Clone)]
pub enum AgenEventAny {
    SessionInit(SessionInitEvent),
    SessionStop(SessionStopEvent),
    StreamDelta(StreamDeltaEvent),
    ToolCall(ToolCallEvent),
    ToolResult(ToolResultEvent),
    AssistantResponse(AssistantResponseEvent),
    SkillLoad(SkillLoadEvent),
    UserPromptSubmit(UserPromptSubmitEvent),
}

impl AgentEvent for AgenEventAny {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        match self {
            Self::SessionInit(s) => s.to_jsonrpc(),
            Self::AssistantResponse(s) => s.to_jsonrpc(),
            Self::SessionStop(s) => s.to_jsonrpc(),
            Self::ToolCall(s) => s.to_jsonrpc(),
            Self::StreamDelta(s) => s.to_jsonrpc(),
            Self::UserPromptSubmit(s) => s.to_jsonrpc(),
            Self::ToolResult(s) => s.to_jsonrpc(),
            Self::SkillLoad(s) => s.to_jsonrpc(),
        }
    }

    fn session_id(self) -> String {
        match self {
            Self::AssistantResponse(s) => s.session_id,
            Self::SessionInit(s) => s.session_id,
            Self::SessionStop(s) => s.session_id,
            Self::StreamDelta(s) => s.session_id,
            Self::SkillLoad(s) => s.session_id,
            Self::ToolCall(s) => s.session_id,
            Self::ToolResult(s) => s.session_id,
            Self::UserPromptSubmit(s) => s.session_id,
        }
    }
}

/// Errors that can occur when parsing a [`JsonRpcNotification`] into an [`AgenEventAny`].
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

impl TryFrom<JsonRpcNotification> for AgenEventAny {
    type Error = AgentEventError;

    fn try_from(value: JsonRpcNotification) -> Result<Self, Self::Error> {
        let session_id = value
            .params
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentEventError::MissingField("session_id".to_string()))?
            .to_string();
        let turn_id = value
            .params
            .get("turn_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match value.method.as_str() {
            "session.init" => Ok(Self::SessionInit(SessionInitEvent {
                session_id,
                model: value
                    .params
                    .get("model")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("model".to_string()))?
                    .to_string(),
                provider: value
                    .params
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("provider".to_string()))?
                    .to_string(),
                system: value
                    .params
                    .get("system")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("system".to_string()))?
                    .to_string(),
                init_type: value
                    .params
                    .get("init_type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("init_type".to_string()))?
                    .to_string(),
            })),
            "session.stop" => Ok(Self::SessionStop(SessionStopEvent {
                session_id,
                success: value
                    .params
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| AgentEventError::InvalidFieldType("success".to_string()))?,
                result: value
                    .params
                    .get("result")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                error: value
                    .params
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })),
            "user.prompt.submit" => Ok(Self::UserPromptSubmit(UserPromptSubmitEvent {
                session_id,
                turn_id,
                prompt: value
                    .params
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("prompt".to_string()))?
                    .to_string(),
            })),
            "stream.delta" => Ok(Self::StreamDelta(StreamDeltaEvent {
                session_id,
                turn_id,
                delta: value
                    .params
                    .get("delta")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("delta".to_string()))?
                    .to_string(),
                delta_type: match value
                    .params
                    .get("delta_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text")
                {
                    "thinking" => DeltaType::Thinking,
                    _ => DeltaType::Text,
                },
            })),
            "tool.call" => Ok(Self::ToolCall(ToolCallEvent {
                session_id,
                turn_id,
                name: value
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("name".to_string()))?
                    .to_string(),
                input: value.params.get("input").cloned().unwrap_or(Value::Null),
            })),
            "tool.result" => {
                let success = value
                    .params
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| AgentEventError::InvalidFieldType("success".to_string()))?;
                let result = value
                    .params
                    .get("result")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let error = value
                    .params
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let tool_result = if success {
                    ToolResult::Ok(result.unwrap_or_default())
                } else {
                    ToolResult::Err(error.unwrap_or_default())
                };
                Ok(Self::ToolResult(ToolResultEvent {
                    session_id,
                    turn_id,
                    result: tool_result,
                }))
            }
            "skill.load" => Ok(Self::SkillLoad(SkillLoadEvent {
                session_id,
                turn_id,
                skill_name: value
                    .params
                    .get("skill_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("skill_name".to_string()))?
                    .to_string(),
            })),
            "assistant.response" => Ok(Self::AssistantResponse(AssistantResponseEvent {
                session_id,
                turn_id,
                full_thinking: value
                    .params
                    .get("full_thinking")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("full_thinking".to_string()))?
                    .to_string(),
                full_text: value
                    .params
                    .get("full_text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentEventError::MissingField("full_text".to_string()))?
                    .to_string(),
            })),
            method => Err(AgentEventError::UnknownMethod(method.to_string())),
        }
    }
}
