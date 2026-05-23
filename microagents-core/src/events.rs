use serde_json::Value;

use crate::types::{AgentEvent, JsonRpcNotification, ToolResult};

pub enum SessionInitType {
    Start,
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

pub enum DeltaType {
    Text,
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

pub struct SessionInitEvent {
    pub session_id: String,
    pub system: String,
    pub init_type: String, // either 'new' or 'resume'
}

pub struct SessionStopEvent {
    pub session_id: String,
    pub success: bool,
    pub result: Option<String>,
    pub error: Option<String>,
}

pub struct UserPromptSubmitEvent {
    pub session_id: String,
    pub turn_id: String,
    pub prompt: String,
}

pub struct StreamDeltaEvent {
    pub session_id: String,
    pub turn_id: String,
    pub delta: String,
    pub delta_type: DeltaType,
}

pub struct ToolCallEvent {
    pub session_id: String,
    pub turn_id: String,
    pub name: String,
    pub input: Value,
}

pub struct ToolResultEvent {
    pub session_id: String,
    pub turn_id: String,
    pub result: ToolResult, // Value implements From<ToolResult> already
}

pub struct SkillLoadEvent {
    pub session_id: String,
    pub turn_id: String,
    pub skill_name: String,
}

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
            .add_param("session_id".into(), Value::from(self.session_id))
            .add_param("system".into(), Value::from(self.system))
    }
}

impl AgentEvent for SessionStopEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("session.stop".into())
            .add_param("session_id".into(), Value::from(self.session_id))
            .add_param("success".into(), Value::from(self.success))
            .add_param("result".into(), Value::from(self.result))
            .add_param("error".into(), Value::from(self.error))
    }
}
