use std::{env::VarError, fmt::Debug, pin::Pin};

use microagents_events::types::ToolResult;
use serde_json::Value;
use thiserror::Error;
use ultrafast_models_sdk::{
    ClientError,
    models::{Function, StreamChunk, Tool},
};

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Generation failed because of {0}")]
    GenerationError(String),
    #[error("Skill resolution failed")]
    SkillResolutionError,
    #[error("Tool call failed because of {0}")]
    ToolCallError(String),
    #[error("Run failure")]
    RunError,
    #[error("Unable to initialize client")]
    ClientInitFailed(#[from] ClientError),
    #[error("API key not configured for provider")]
    ApiKeyNotConfigured(#[from] VarError),
}

pub type StreamItem = Result<StreamChunk, AgentError>;
pub type GenerationStream = Pin<Box<dyn futures_core::Stream<Item = StreamItem> + Send>>;

#[async_trait::async_trait]
pub trait Agent {
    async fn generate(mut self) -> Result<GenerationStream, AgentError>;
    async fn call_tool(self, tool_name: &str, tool_args: &str) -> Result<ToolResult, AgentError>;
    async fn run(self, prompt: String) -> Result<(), AgentError>;
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
        ctx: &ToolExecutionContext<Ctx>,
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
