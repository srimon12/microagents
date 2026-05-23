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

impl AgentEvent for UserPromptSubmitEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("user.prompt.submit".into())
            .add_param("session_id".into(), Value::from(self.session_id))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("prompt".into(), Value::from(self.prompt))
    }
}

impl AgentEvent for StreamDeltaEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("stream.delta".into())
            .add_param("session_id".into(), Value::from(self.session_id))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("delta".into(), Value::from(self.delta))
            .add_param("delta_type".into(), Value::from(self.delta_type))
    }
}

impl AgentEvent for ToolCallEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("tool.call".into())
            .add_param("session_id".into(), Value::from(self.session_id))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("name".into(), Value::from(self.name))
            .add_param("input".into(), Value::from(self.input))
    }
}

impl AgentEvent for ToolResultEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("tool.result".into())
            .add_param("session_id".into(), Value::from(self.session_id))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("result".into(), Value::from(self.result))
    }
}

impl AgentEvent for SkillLoadEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("skill.load".into())
            .add_param("session_id".into(), Value::from(self.session_id))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("skill_name".into(), Value::from(self.skill_name))
    }
}

impl AgentEvent for AssistantResponseEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("assistant.response".into())
            .add_param("session_id".into(), Value::from(self.session_id))
            .add_param("turn_id".into(), Value::from(self.turn_id))
            .add_param("full_thinking".into(), Value::from(self.full_thinking))
            .add_param("full_text".into(), Value::from(self.full_text))
    }
}
