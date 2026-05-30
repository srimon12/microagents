use clap::Parser;
use microagents_core::{
    agent::{MicroAgentBuilder, SupportedProvider},
    types::{Agent, AgentError, ToolExecutionContext},
};
use microagents_storage::types::AgentStorageChoice;
use std::{str::FromStr, sync::Arc};

use crate::init_env::initialize_environment;

mod init_env;
mod search;
mod tools;
mod tui;

/// CLI Agent built on top of the MicroAgents framework
#[derive(Parser, Debug)]
#[command(version = "0.1.0")]
#[command(name = "microag")]
#[command(about, long_about = None)]
struct Args {
    /// LLM model to use (must match with the one offered by the provider).
    /// Falls back to the provider's default model if not provided
    #[arg(long, default_value = None)]
    model: Option<String>,

    /// Add one or more local/global skills by providing their name.
    /// Falls back to skills auto-discovery if no skill has been provided.
    #[arg(long)]
    skill: Vec<String>,

    /// Provider to use for the agent.
    /// Falls back to 'openrouter' if not provided.
    #[arg(long, default_value = None)]
    provider: Option<String>,

    /// Storage backend for sessions peristence. Allowed values: 'jsonl', 'sqlite'.
    /// Defaults to 'jsonl' (store session events as newline-separated JSON objects in a file)
    #[arg(long, default_value = None)]
    storage: Option<String>,

    /// Resume a previous session by id. If omitted, a new session is started.
    #[arg(long = "session-id", value_name = "ID")]
    session_id: Option<String>,
}

async fn build_agent(
    provider: Option<String>,
    model: Option<String>,
    storage: Option<String>,
    skills: Vec<String>,
) -> Result<microagents_core::agent::MicroAgent<()>, AgentError> {
    let st = match storage {
        Some(s) => match s.as_str() {
            "jsonl" => AgentStorageChoice::Jsonl,
            "sqlite" => AgentStorageChoice::Sqlite,
            _ => {
                return Err(AgentError::ClientInitFailed(
                    "Invalid storage type: {s}".into(),
                ));
            }
        },
        None => AgentStorageChoice::Jsonl,
    };
    let prov = SupportedProvider::from_str(&provider.clone().unwrap_or("openrouter".to_string()))
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?;
    let base_builder = MicroAgentBuilder::<()>::new(ToolExecutionContext::new(()))
        .model(model.unwrap_or(prov.default_model().to_string()))
        .provider(provider.unwrap_or("openrouter".into()))
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .storage(st)
        .await
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
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?;
    let base_builder = if skills.is_empty() {
        base_builder
            .find_skills()
            .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
    } else {
        let mut builder = base_builder;
        for skill in skills {
            builder = builder
                .add_skill(skill)
                .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?;
        }
        builder
    };
    Ok(base_builder.build())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_environment().await?;
    println!("Launching TUI...");
    let args = Args::parse();
    let initial_session = args.session_id.clone();
    tui::run_with_session(initial_session, move |prompt, session_id| {
        let prov_c = args.provider.clone();
        let model_c = args.model.clone();
        let skill_c = args.skill.clone();
        let storage_c = args.storage.clone();
        async move {
            let agent = build_agent(prov_c, model_c, storage_c, skill_c).await?;
            agent.run(prompt, session_id).await
        }
    })
    .await?;
    Ok(())
}
