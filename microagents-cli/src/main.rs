use microagents_core::{
    agent::MicroAgentBuilder,
    types::{Agent, AgentError, ToolExecutionContext},
};
use microagents_storage::types::AgentStorageChoice;
use std::sync::Arc;

use crate::init_env::initialize_environment;

mod init_env;
mod search;
mod tools;
mod tui;

fn build_agent() -> Result<microagents_core::agent::MicroAgent<()>, AgentError> {
    Ok(MicroAgentBuilder::<()>::new(ToolExecutionContext::new(()))
        .model("anthropic/claude-opus-4.7".into())
        .provider("openrouter".into())
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .storage(AgentStorageChoice::Jsonl)
        .find_skills()
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .add_tool(Arc::new(tools::WriteTool))
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .add_tool(Arc::new(tools::EditTool))
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .add_tool(Arc::new(tools::ShellExecuteTool))
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .add_tool(Arc::new(tools::SearchTool))
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .add_tool(Arc::new(tools::ReadTool))
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .build())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_environment().await?;
    println!("Launching TUI...");
    tui::run(|prompt, session_id| async move {
        let agent = build_agent()?;
        agent.run(prompt, session_id).await
    })
    .await?;
    Ok(())
}
