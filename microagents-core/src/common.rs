use std::sync::Arc;

use microagents_events::{AgentEventAny, types::ToolResult};
use serde_json::Value;
use ultrafast_models_sdk::{
    Message, Role,
    models::{FunctionCall, ToolCall},
};

use crate::types::{AgentError, ToolExecutionContext, ToolFunction};

pub fn convert_event_to_message(event: AgentEventAny) -> Option<Message> {
    let message: Option<Message> = match event {
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
            };
            Some(Message {
                role: Role::User,
                content: result,
                name: None,
                tool_calls: None,
                tool_call_id: Some(p.tool_call_id),
            })
        }
        _ => None,
    };

    message
}

pub enum JsonResult {
    Valid(Value),
    Incomplete,
    Malformed,
}

pub fn is_valid_json(s: &str) -> JsonResult {
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

pub async fn call_tool<Ctx: Send + Sync + 'static>(
    tool: Arc<dyn ToolFunction<Ctx>>,
    tool_args: Value,
    tool_context: Arc<ToolExecutionContext<Ctx>>,
) -> Result<ToolResult, AgentError> {
    jsonschema::validate(&tool.input_schema(), &tool_args)
        .map_err(|e| AgentError::ToolCallError(e.to_string()))?;
    let result = tool.execute(tool_args, &tool_context).await?;
    return Ok(result);
}
