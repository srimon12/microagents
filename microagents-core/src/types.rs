use microagents_events::{AgentEventAny, types::ToolResult};
use serde_json::Value;
use std::{env::VarError, fmt::Debug, pin::Pin, sync::Arc};
use thiserror::Error;
use ultrafast_models_sdk::models::{Function, StreamChunk, Tool};

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Generation failed because of {0}")]
    GenerationError(String),
    #[error("Skill resolution failed")]
    SkillResolutionError,
    #[error("Tool call failed because of {0}")]
    ToolCallError(String),
    #[error("Run failure: {0}")]
    RunError(String),
    #[error("Unable to initialize client: {0}")]
    ClientInitFailed(String),
    #[error("API key not configured for provider")]
    ApiKeyNotConfigured(#[from] VarError),
    #[error("Session load error: {0}")]
    SessionLoadError(String),
    #[error("Conversion error from agent event to message")]
    EventConversionError,
}

pub type StreamItem = Result<StreamChunk, AgentError>;
pub type GenerationStream = Pin<Box<dyn futures_core::Stream<Item = StreamItem> + Send>>;
pub type RunStreamItem = Result<AgentEventAny, AgentError>;
pub type RunStream = Pin<Box<dyn futures_core::Stream<Item = RunStreamItem> + Send>>;

#[async_trait::async_trait]
pub trait Agent: Send + Sync {
    async fn generate(&mut self) -> Result<GenerationStream, AgentError>;
    async fn run(
        mut self,
        prompt: String,
        session_id: Option<String>,
    ) -> Result<RunStream, AgentError>;
}

#[derive(Debug)]
pub struct ToolExecutionContext<Ctx> {
    pub context: Ctx,
}

impl<Ctx> ToolExecutionContext<Ctx> {
    pub fn new(context: Ctx) -> Self {
        Self { context }
    }
}

impl<Ctx: Default> Default for ToolExecutionContext<Ctx> {
    fn default() -> Self {
        Self::new(Ctx::default())
    }
}

#[async_trait::async_trait]
pub trait ToolFunction<Ctx>: Debug + Send + Sync {
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn input_schema(&self) -> Value;
    async fn execute(
        &self,
        input: Value,
        ctx: &Arc<ToolExecutionContext<Ctx>>,
    ) -> Result<ToolResult, AgentError>;
    fn to_sdk_tool(&self) -> Tool {
        Tool {
            tool_type: "function".into(),
            function: Function {
                name: self.name(),
                description: Some(self.description()),
                parameters: self.input_schema(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[derive(Debug)]
    struct MyTool;

    #[async_trait::async_trait]
    impl ToolFunction<()> for MyTool {
        fn name(&self) -> String {
            "tool".to_string()
        }

        fn description(&self) -> String {
            "description".to_string()
        }

        fn input_schema(&self) -> Value {
            json!({})
        }

        async fn execute(
            &self,
            _input: Value,
            _ctx: &Arc<ToolExecutionContext<()>>,
        ) -> Result<ToolResult, AgentError> {
            Ok(ToolResult::Ok("success".into()))
        }
    }

    #[test]
    fn test_to_sdk_tool() {
        let tool = MyTool {};
        let sdk_tool = tool.to_sdk_tool();
        assert_eq!(sdk_tool.tool_type, "function".to_string());
        assert_eq!(sdk_tool.function.name, tool.name());
        assert_eq!(sdk_tool.function.description, Some(tool.description()));
        assert_eq!(sdk_tool.function.parameters, tool.input_schema());
    }
}
