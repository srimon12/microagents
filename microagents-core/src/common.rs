use microagents_events::{AgentEventAny, types::ToolResult};
use ultrafast_models_sdk::{
    Message, Role,
    models::{FunctionCall, ToolCall},
};

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
