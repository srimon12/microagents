#[cfg(feature = "token_estimation")]
use std::sync::OnceLock;
use std::{fs, sync::Arc};

use microagents_events::{AgentEventAny, types::ToolResult};
use serde_json::Value;
use ultrafast_models_sdk::{
    Message, Role,
    models::{FunctionCall, ToolCall},
};

use crate::{
    agent::MicroAgentBuilderError,
    types::{AgentError, ToolExecutionContext, ToolFunction},
};

const AGENTS_MD: &str = "AGENTS.md";

#[cfg(feature = "token_estimation")]
pub static TOKENIZER: OnceLock<Result<tokie::Tokenizer, tokie::HubError>> = OnceLock::new();

#[cfg(feature = "token_estimation")]
pub fn tokenizer() -> &'static Result<tokie::Tokenizer, tokie::HubError> {
    TOKENIZER.get_or_init(|| tokie::Tokenizer::from_pretrained("codellama/CodeLlama-7b-hf"))
}

/// Verify that an environment variable is set.
///
/// Returns `Ok(())` if the variable exists, otherwise propagates the [`VarError`].
pub fn check_env_var(api_key: &str) -> Result<(), std::env::VarError> {
    let _ = std::env::var(api_key)?;
    Ok(())
}

/// Convert a persisted [`AgentEventAny`] back into an SDK [`Message`].
///
/// Only events that correspond to chat roles (`User`, `Assistant`, `Tool`)
/// produce a message. All other variants return [`None`].
pub fn convert_event_to_message(event: AgentEventAny) -> Option<Message> {
    match event {
        AgentEventAny::UserPromptSubmit(p) => Some(Message {
            role: Role::User,
            content: p.prompt,
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }),
        AgentEventAny::AssistantResponse(p) => {
            let msg = if let Some(tc) = p.tool_calls {
                let calls: Vec<ToolCall> = tc
                    .iter()
                    .map(|t| ToolCall {
                        call_type: t.call_type.clone(),
                        id: t.id.clone(),
                        function: FunctionCall {
                            name: t.function.name.clone(),
                            arguments: t.function.arguments.clone(),
                        },
                    })
                    .collect();
                Message {
                    role: Role::Assistant,
                    content: p.full_text,
                    name: None,
                    tool_calls: Some(calls),
                    tool_call_id: None,
                }
            } else {
                Message {
                    role: Role::Assistant,
                    content: p.full_text,
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                }
            };
            Some(msg)
        }
        AgentEventAny::ToolResult(p) => {
            let result = match p.result {
                ToolResult::Ok(r) => format!("Tool call succeeded: {}", r),
                ToolResult::Err(r) => format!("Tool call failed: {}", r),
                _ => unreachable!("ToolResult should not reach this branch"),
            };
            Some(Message {
                role: Role::Tool,
                content: result,
                name: None,
                tool_calls: None,
                tool_call_id: Some(p.tool_call_id),
            })
        }
        _ => None,
    }
}

/// Result of attempting to parse a (potentially partial) JSON string.
pub enum JsonResult {
    /// Fully valid JSON value.
    Valid(Value),
    /// The input is a valid prefix but truncated (EOF while parsing).
    Incomplete,
    /// The input is not valid JSON.
    Malformed,
}

/// Parse a JSON string that may be incomplete (e.g. streaming tool arguments).
///
/// Returns [`JsonResult::Incomplete`] when the payload is cut off mid-token,
/// allowing the caller to buffer and retry.
pub fn parse_json_fragment(s: &str) -> JsonResult {
    let v = serde_json::from_str::<Value>(s);
    match v {
        Ok(val) => JsonResult::Valid(val),
        Err(e) => {
            if e.is_eof() {
                return JsonResult::Incomplete;
            }
            JsonResult::Malformed
        }
    }
}

/// Validate tool arguments against its JSON schema and then execute it.
///
/// This is the canonical entry-point for invoking a [`ToolFunction`] from the
/// agent runtime. It first checks schema conformance with `jsonschema`, then
/// calls [`ToolFunction::execute`].
pub async fn call_tool<Ctx: Send + Sync + 'static>(
    tool: Arc<dyn ToolFunction<Ctx>>,
    tool_args: Value,
    tool_context: Arc<ToolExecutionContext<Ctx>>,
) -> Result<ToolResult, AgentError> {
    jsonschema::validate(&tool.input_schema(), &tool_args)
        .map_err(|e| AgentError::ToolCallError(e.to_string()))?;
    let result = tool.execute(tool_args, &tool_context).await?;
    Ok(result)
}

/// Estimate the number of tokens in a given text using the GPT-2 tokenizer.
/// Requires the `token_estimation` feature. Returns 0 if the feature is disabled.
pub fn estimate_tokens(_text: &str) -> Result<usize, AgentError> {
    #[cfg(feature = "token_estimation")]
    {
        Ok(tokenizer()
            .as_ref()
            .map_err(|e| AgentError::TokenizerLoadingError(e.to_string()))?
            .count_tokens(_text))
    }
    #[cfg(not(feature = "token_estimation"))]
    {
        Ok(0)
    }
}

pub fn load_agents_md() -> Result<String, MicroAgentBuilderError> {
    let content = fs::read_to_string(AGENTS_MD)?;
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use microagents_events::{
        AssistantResponseEvent, SessionInitEvent, SessionInitType, SessionStopEvent,
        SkillLoadEvent, StreamDeltaEvent, ToolCallEvent, ToolResultEvent, Usage,
        UserPromptSubmitEvent,
        types::{FunctionCall as EventFunctionCall, ToolCall as EventToolCall},
    };

    #[test]
    fn test_convert_user_prompt_submit() {
        let event = AgentEventAny::UserPromptSubmit(UserPromptSubmitEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            prompt: "hello".into(),
            timestamp: Utc::now(),
        });
        let msg = convert_event_to_message(event).unwrap();
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "hello");
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_convert_assistant_response_without_tool_calls() {
        let event = AgentEventAny::AssistantResponse(AssistantResponseEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            full_text: "hi there".into(),
            tool_calls: None,
            timestamp: Utc::now(),
        });
        let msg = convert_event_to_message(event).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content, "hi there");
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_convert_assistant_response_with_tool_calls() {
        let event = AgentEventAny::AssistantResponse(AssistantResponseEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            full_text: "calling tool".into(),
            tool_calls: Some(vec![EventToolCall {
                id: "tc1".into(),
                call_type: "function".into(),
                function: EventFunctionCall {
                    name: "my_tool".into(),
                    arguments: "{\"x\":1}".into(),
                },
            }]),
            timestamp: Utc::now(),
        });
        let msg = convert_event_to_message(event).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "tc1");
        assert_eq!(calls[0].function.name, "my_tool");
        assert_eq!(calls[0].function.arguments, "{\"x\":1}");
    }

    #[test]
    fn test_convert_tool_result_ok() {
        let event = AgentEventAny::ToolResult(ToolResultEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            result: ToolResult::Ok("done".into()),
            tool_call_id: "tc1".into(),
            timestamp: Utc::now(),
        });
        let msg = convert_event_to_message(event).unwrap();
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.content, "Tool call succeeded: done");
        assert_eq!(msg.tool_call_id, Some("tc1".into()));
    }

    #[test]
    fn test_convert_tool_result_err() {
        let event = AgentEventAny::ToolResult(ToolResultEvent {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            result: ToolResult::Err("oops".into()),
            tool_call_id: "tc2".into(),
            timestamp: Utc::now(),
        });
        let msg = convert_event_to_message(event).unwrap();
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.content, "Tool call failed: oops");
        assert_eq!(msg.tool_call_id, Some("tc2".into()));
    }

    #[test]
    fn test_convert_other_events_return_none() {
        assert!(
            convert_event_to_message(AgentEventAny::SessionInit(SessionInitEvent {
                session_id: "s1".into(),
                model: "m".into(),
                provider: "p".into(),
                system: "sys".into(),
                init_type: SessionInitType::Start,
                timestamp: Utc::now(),
            }))
            .is_none()
        );

        assert!(
            convert_event_to_message(AgentEventAny::SessionStop(SessionStopEvent {
                session_id: "s1".into(),
                success: true,
                result: None,
                error: None,
                timestamp: Utc::now(),
                usage: Usage::default()
            }))
            .is_none()
        );

        assert!(
            convert_event_to_message(AgentEventAny::StreamDelta(StreamDeltaEvent {
                session_id: "s1".into(),
                turn_id: "t1".into(),
                delta: "d".into(),
                delta_type: microagents_events::DeltaType::Text,
                timestamp: Utc::now(),
            }))
            .is_none()
        );

        assert!(
            convert_event_to_message(AgentEventAny::ToolCall(ToolCallEvent {
                session_id: "s1".into(),
                turn_id: "t1".into(),
                name: "tool".into(),
                input: Value::Null,
                timestamp: Utc::now(),
            }))
            .is_none()
        );

        assert!(
            convert_event_to_message(AgentEventAny::SkillLoad(SkillLoadEvent {
                session_id: "s1".into(),
                turn_id: "t1".into(),
                skill_name: "skill".into(),
                timestamp: Utc::now(),
            }))
            .is_none()
        );
    }

    #[test]
    fn test_parse_json_fragment_valid() {
        match parse_json_fragment(r#"{"key": "value"}"#) {
            JsonResult::Valid(v) => assert_eq!(v["key"], "value"),
            _ => panic!("expected Valid"),
        }
    }

    #[test]
    fn test_parse_json_fragment_incomplete() {
        match parse_json_fragment(r#"{"key": "val""#) {
            JsonResult::Incomplete => {}
            _ => panic!("expected Incomplete"),
        }
    }

    #[test]
    fn test_parse_json_fragment_malformed() {
        match parse_json_fragment(r#"{"key": "value",}"#) {
            JsonResult::Malformed => {}
            _ => panic!("expected Malformed"),
        }
    }

    #[derive(Debug)]
    struct DummyTool {
        schema: Value,
    }

    #[async_trait::async_trait]
    impl ToolFunction<()> for DummyTool {
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn description(&self) -> &'static str {
            "desc"
        }
        fn input_schema(&self) -> Value {
            self.schema.clone()
        }
        async fn execute(
            &self,
            _input: Value,
            _ctx: &Arc<ToolExecutionContext<()>>,
        ) -> Result<ToolResult, AgentError> {
            Ok(ToolResult::Ok("ok".into()))
        }
    }

    #[tokio::test]
    async fn test_call_tool_validates_and_executes() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let tool = Arc::new(DummyTool { schema });
        let ctx = Arc::new(ToolExecutionContext::new(()));
        let args = serde_json::json!({"name": "world"});
        let result = call_tool(tool, args, ctx).await.unwrap();
        assert!(matches!(result, ToolResult::Ok(ref s) if s == "ok"));
    }

    #[tokio::test]
    async fn test_call_tool_schema_validation_fails() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            },
            "required": ["count"]
        });
        let tool = Arc::new(DummyTool { schema });
        let ctx = Arc::new(ToolExecutionContext::new(()));
        let args = serde_json::json!({"count": "not a number"});
        let err = call_tool(tool, args, ctx).await.unwrap_err();
        match err {
            AgentError::ToolCallError(_) => {}
            other => panic!("expected ToolCallError, got {:?}", other),
        }
    }

    #[test]
    #[cfg(feature = "token_estimation")]
    fn test_estimate_tokens() {
        let count = estimate_tokens("hello world").expect("Should be able to estimate tokens");
        assert_eq!(count, 2);
    }

    #[test]
    #[cfg(not(feature = "token_estimation"))]
    fn test_estimate_tokens() {
        let count = estimate_tokens("hello world").expect("Should be able to estimate tokens");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_load_agents_md() {
        let instr = "you are a helpful assistant";
        fs::write(AGENTS_MD, instr).expect("Should be able to write file");
        let loaded = load_agents_md().expect("Should be able to load AGENTS.md content");
        assert_eq!(loaded, instr);
    }
}
