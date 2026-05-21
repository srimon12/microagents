use thiserror::Error;
use ultrafast_models_sdk::Message;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Generation failed")]
    GenerationError,
    #[error("Skill resolution failed")]
    SkillResolutionError,
    #[error("Tool call failed")]
    ToolCallError,
    #[error("Run failure")]
    RunError,
}

#[async_trait::async_trait]
pub trait Agent {
    async fn generate(self, history: Vec<Message>) -> Result<Message, AgentError>;
    async fn call_tool(self, tool_name: &str, tool_args: &str) -> Result<Message, AgentError>;
    async fn resolve_skill(self, skill_name: &str) -> Result<Message, AgentError>;
    async fn run(self, prompt: String) -> Result<(), AgentError>;
}
