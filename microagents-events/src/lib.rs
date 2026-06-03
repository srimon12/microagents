pub mod types;

use serde_json::Value;

use std::{convert::TryFrom, str::FromStr};

use crate::types::{AgentEvent, AgentEventError, JsonRpcNotification, ToolCall, ToolResult};

/// Indicates whether a session is being started fresh or resumed.
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl FromStr for SessionInitType {
    type Err = AgentEventError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "start" => Ok(Self::Start),
            "resume" => Ok(Self::Resume),
            _ => Err(AgentEventError::InvalidFieldType(format!(
                "No init type with message: {}",
                s
            ))),
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
    pub init_type: SessionInitType,
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
    pub tool_call_id: String,
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
    pub full_text: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl AgentEvent for SessionInitEvent {
    fn to_jsonrpc(self) -> JsonRpcNotification {
        JsonRpcNotification::builder()
            .method("session.init".into())
            .add_param("session_id".into(), Value::from(self.session_id.clone()))
            .add_param("system".into(), Value::from(self.system))
            .add_param("model".into(), Value::from(self.model))
            .add_param("provider".into(), Value::from(self.provider))
            .add_param("init_type".into(), Value::from(self.init_type))
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
            .add_param("input".into(), self.input)
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
            .add_param("tool_call_id".into(), Value::from(self.tool_call_id))
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
            .add_param("full_text".into(), Value::from(self.full_text))
            .add_param("tool_calls".into(), Value::from(self.tool_calls))
    }

    fn session_id(self) -> String {
        self.session_id
    }
}

/// A sum type wrapping any agent event.
#[derive(Debug, Clone)]
pub enum AgentEventAny {
    SessionInit(SessionInitEvent),
    SessionStop(SessionStopEvent),
    StreamDelta(StreamDeltaEvent),
    ToolCall(ToolCallEvent),
    ToolResult(ToolResultEvent),
    AssistantResponse(AssistantResponseEvent),
    SkillLoad(SkillLoadEvent),
    UserPromptSubmit(UserPromptSubmitEvent),
}

impl AgentEvent for AgentEventAny {
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

impl TryFrom<JsonRpcNotification> for AgentEventAny {
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
                    .and_then(|v| match v.as_str() {
                        Some(s) => SessionInitType::from_str(s).ok(),
                        None => None,
                    })
                    .ok_or_else(|| AgentEventError::MissingField("init_type".to_string()))?,
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
                    .get("result")
                    .and_then(|v| v["success"].as_bool())
                    .ok_or_else(|| AgentEventError::InvalidFieldType("success".to_string()))?;
                let result = value
                    .params
                    .get("result")
                    .and_then(|v| v["result"].as_str())
                    .map(|s| s.to_string());
                let error = value
                    .params
                    .get("result")
                    .and_then(|v| v["error"].as_str())
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
                    tool_call_id: value
                        .params
                        .get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| AgentEventError::MissingField("tool_call_id".to_string()))?
                        .to_string(),
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
            "assistant.response" => {
                let tool_calls = value.params.get("tool_calls").and_then(|v| match v {
                    Value::Array(arr) => arr
                        .iter()
                        .map(|a| ToolCall::try_from(a.clone()))
                        .collect::<Result<Vec<_>, _>>()
                        .ok(),
                    _ => None,
                });
                Ok(Self::AssistantResponse(AssistantResponseEvent {
                    session_id,
                    turn_id,
                    full_text: value
                        .params
                        .get("full_text")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| AgentEventError::MissingField("full_text".to_string()))?
                        .to_string(),
                    tool_calls,
                }))
            }
            method => Err(AgentEventError::UnknownMethod(method.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_init_type_from_value() {
        assert_eq!(Value::from(SessionInitType::Start), Value::from("start"));
        assert_eq!(Value::from(SessionInitType::Resume), Value::from("resume"));
    }

    #[test]
    fn session_init_type_from_str_ok() {
        assert!(matches!(
            SessionInitType::from_str("start"),
            Ok(SessionInitType::Start)
        ));
        assert!(matches!(
            SessionInitType::from_str("resume"),
            Ok(SessionInitType::Resume)
        ));
    }

    #[test]
    fn session_init_type_from_str_err() {
        let err = SessionInitType::from_str("unknown").unwrap_err();
        assert!(matches!(err, AgentEventError::InvalidFieldType(_)));
        assert!(
            err.to_string()
                .contains("No init type with message: unknown")
        );
    }

    #[test]
    fn delta_type_from_value() {
        assert_eq!(Value::from(DeltaType::Text), Value::from("text"));
        assert_eq!(Value::from(DeltaType::Thinking), Value::from("thinking"));
    }

    #[test]
    fn session_init_event_to_jsonrpc() {
        let event = SessionInitEvent {
            session_id: "s1".into(),
            model: "gpt-4".into(),
            provider: "openai".into(),
            system: "sys".into(),
            init_type: SessionInitType::Start,
        };
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "session.init");
        assert_eq!(rpc.params.get("session_id"), Some(&Value::from("s1")));
        assert_eq!(rpc.params.get("model"), Some(&Value::from("gpt-4")));
        assert_eq!(rpc.params.get("provider"), Some(&Value::from("openai")));
        assert_eq!(rpc.params.get("system"), Some(&Value::from("sys")));
        assert_eq!(rpc.params.get("init_type"), Some(&Value::from("start")));
    }

    #[test]
    fn session_stop_event_to_jsonrpc() {
        let event = SessionStopEvent {
            session_id: "s1".into(),
            success: true,
            result: Some("done".into()),
            error: None,
        };
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "session.stop");
        assert_eq!(rpc.params.get("success"), Some(&Value::from(true)));
        assert_eq!(rpc.params.get("result"), Some(&Value::from("done")));
        assert_eq!(rpc.params.get("error"), Some(&Value::Null));
    }

    #[test]
    fn user_prompt_submit_event_to_jsonrpc() {
        let event = UserPromptSubmitEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            prompt: "hello".into(),
        };
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "user.prompt.submit");
        assert_eq!(rpc.params.get("prompt"), Some(&Value::from("hello")));
    }

    #[test]
    fn stream_delta_event_to_jsonrpc() {
        let event = StreamDeltaEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            delta: "world".into(),
            delta_type: DeltaType::Thinking,
        };
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "stream.delta");
        assert_eq!(rpc.params.get("delta"), Some(&Value::from("world")));
        assert_eq!(rpc.params.get("delta_type"), Some(&Value::from("thinking")));
    }

    #[test]
    fn tool_call_event_to_jsonrpc() {
        let event = ToolCallEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            name: "read".into(),
            input: json!({"path": "/tmp"}),
        };
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "tool.call");
        assert_eq!(rpc.params.get("name"), Some(&Value::from("read")));
        assert_eq!(rpc.params.get("input"), Some(&json!({"path": "/tmp"})));
    }

    #[test]
    fn tool_result_event_to_jsonrpc() {
        let event = ToolResultEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            result: ToolResult::Ok("ok".into()),
            tool_call_id: "tc1".into(),
        };
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "tool.result");
        assert_eq!(rpc.params.get("tool_call_id"), Some(&Value::from("tc1")));
        assert_eq!(
            rpc.params.get("result"),
            Some(&json!({"success": true, "result": "ok", "error": Value::Null}))
        );
    }

    #[test]
    fn skill_load_event_to_jsonrpc() {
        let event = SkillLoadEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            skill_name: "coding".into(),
        };
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "skill.load");
        assert_eq!(rpc.params.get("skill_name"), Some(&Value::from("coding")));
    }

    #[test]
    fn assistant_response_event_to_jsonrpc() {
        let event = AssistantResponseEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            full_text: "hi".into(),
            tool_calls: None,
        };
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "assistant.response");
        assert_eq!(rpc.params.get("full_text"), Some(&Value::from("hi")));
        assert_eq!(rpc.params.get("tool_calls"), Some(&Value::Null));
    }

    #[test]
    fn agent_event_any_session_id() {
        let event = AgentEventAny::SessionInit(SessionInitEvent {
            session_id: "sid".into(),
            model: "m".into(),
            provider: "p".into(),
            system: "s".into(),
            init_type: SessionInitType::Start,
        });
        assert_eq!(event.session_id(), "sid");
    }

    #[test]
    fn agent_event_any_to_jsonrpc_roundtrip() {
        let event = AgentEventAny::UserPromptSubmit(UserPromptSubmitEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            prompt: "p".into(),
        });
        let rpc = event.to_jsonrpc();
        assert_eq!(rpc.method, "user.prompt.submit");
        assert_eq!(rpc.params.get("session_id"), Some(&Value::from("s1")));
    }

    #[test]
    fn try_from_jsonrpc_session_init_ok() {
        let rpc = JsonRpcNotification::builder()
            .method("session.init".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("model".into(), Value::from("gpt-4"))
            .add_param("provider".into(), Value::from("openai"))
            .add_param("system".into(), Value::from("sys"))
            .add_param("init_type".into(), Value::from("resume"));
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(
            matches!(any, AgentEventAny::SessionInit(ref e) if e.session_id == "s1" && matches!(e.init_type, SessionInitType::Resume))
        );
    }

    #[test]
    fn try_from_jsonrpc_session_init_missing_field() {
        let rpc = JsonRpcNotification::builder()
            .method("session.init".into())
            .add_param("session_id".into(), Value::from("s1"));
        let err = AgentEventAny::try_from(rpc).unwrap_err();
        assert!(matches!(err, AgentEventError::MissingField(_)));
    }

    #[test]
    fn try_from_jsonrpc_session_init_invalid_init_type() {
        let rpc = JsonRpcNotification::builder()
            .method("session.init".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("model".into(), Value::from("gpt-4"))
            .add_param("provider".into(), Value::from("openai"))
            .add_param("system".into(), Value::from("sys"))
            .add_param("init_type".into(), Value::from("invalid"));
        let err = AgentEventAny::try_from(rpc).unwrap_err();
        assert!(matches!(err, AgentEventError::InvalidFieldType(_)));
    }

    #[test]
    fn try_from_jsonrpc_session_stop_ok() {
        let rpc = JsonRpcNotification::builder()
            .method("session.stop".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("success".into(), Value::from(true))
            .add_param("result".into(), Value::from("done"))
            .add_param("error".into(), Value::Null);
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(
            matches!(any, AgentEventAny::SessionStop(ref e) if e.success && e.result == Some("done".into()) && e.error.is_none())
        );
    }

    #[test]
    fn try_from_jsonrpc_user_prompt_submit_ok() {
        let rpc = JsonRpcNotification::builder()
            .method("user.prompt.submit".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("turn_id".into(), Value::from("t1"))
            .add_param("prompt".into(), Value::from("hello"));
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(matches!(any, AgentEventAny::UserPromptSubmit(ref e) if e.prompt == "hello"));
    }

    #[test]
    fn try_from_jsonrpc_stream_delta_text_default() {
        let rpc = JsonRpcNotification::builder()
            .method("stream.delta".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("turn_id".into(), Value::from("t1"))
            .add_param("delta".into(), Value::from("d"));
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(
            matches!(any, AgentEventAny::StreamDelta(ref e) if e.delta == "d" && matches!(e.delta_type, DeltaType::Text))
        );
    }

    #[test]
    fn try_from_jsonrpc_stream_delta_thinking() {
        let rpc = JsonRpcNotification::builder()
            .method("stream.delta".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("turn_id".into(), Value::from("t1"))
            .add_param("delta".into(), Value::from("d"))
            .add_param("delta_type".into(), Value::from("thinking"));
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(
            matches!(any, AgentEventAny::StreamDelta(ref e) if matches!(e.delta_type, DeltaType::Thinking))
        );
    }

    #[test]
    fn try_from_jsonrpc_tool_call_ok() {
        let rpc = JsonRpcNotification::builder()
            .method("tool.call".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("turn_id".into(), Value::from("t1"))
            .add_param("name".into(), Value::from("read"))
            .add_param("input".into(), json!({"path": "/tmp"}));
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(matches!(any, AgentEventAny::ToolCall(ref e) if e.name == "read"));
    }

    #[test]
    fn try_from_jsonrpc_tool_result_ok() {
        let rpc = JsonRpcNotification::builder()
            .method("tool.result".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("turn_id".into(), Value::from("t1"))
            .add_param("tool_call_id".into(), Value::from("tc1"))
            .add_param(
                "result".into(),
                json!({"success": true, "result": "ok", "error": Value::Null}),
            );
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(
            matches!(any, AgentEventAny::ToolResult(ref e) if matches!(e.result, ToolResult::Ok(ref s) if s == "ok"))
        );
    }

    #[test]
    fn try_from_jsonrpc_skill_load_ok() {
        let rpc = JsonRpcNotification::builder()
            .method("skill.load".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("turn_id".into(), Value::from("t1"))
            .add_param("skill_name".into(), Value::from("coding"));
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(matches!(any, AgentEventAny::SkillLoad(ref e) if e.skill_name == "coding"));
    }

    #[test]
    fn try_from_jsonrpc_assistant_response_ok() {
        let rpc = JsonRpcNotification::builder()
            .method("assistant.response".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("turn_id".into(), Value::from("t1"))
            .add_param("full_text".into(), Value::from("hi"));
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(
            matches!(any, AgentEventAny::AssistantResponse(ref e) if e.full_text == "hi" && e.tool_calls.is_none())
        );
    }

    #[test]
    fn try_from_jsonrpc_assistant_response_with_tool_calls() {
        let rpc = JsonRpcNotification::builder()
            .method("assistant.response".into())
            .add_param("session_id".into(), Value::from("s1"))
            .add_param("turn_id".into(), Value::from("t1"))
            .add_param("full_text".into(), Value::from("hi"))
            .add_param("tool_calls".into(), json!([{"call_type":"function","id":"1","function":{"name":"tool","arguments":"{}"}}]));
        let any = AgentEventAny::try_from(rpc).unwrap();
        assert!(matches!(any, AgentEventAny::AssistantResponse(ref e) if e.tool_calls.is_some()));
    }

    #[test]
    fn try_from_jsonrpc_unknown_method() {
        let rpc = JsonRpcNotification::builder()
            .method("unknown".into())
            .add_param("session_id".into(), Value::from("s1"));
        let err = AgentEventAny::try_from(rpc).unwrap_err();
        assert!(matches!(err, AgentEventError::UnknownMethod(ref m) if m == "unknown"));
    }

    #[test]
    fn try_from_jsonrpc_missing_session_id() {
        let rpc = JsonRpcNotification::builder()
            .method("session.stop".into())
            .add_param("success".into(), Value::from(true));
        let err = AgentEventAny::try_from(rpc).unwrap_err();
        assert!(matches!(err, AgentEventError::MissingField(ref m) if m == "session_id"));
    }
}
