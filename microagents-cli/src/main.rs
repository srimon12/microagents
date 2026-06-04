use clap::Parser;
use futures_util::StreamExt;
use microagents_core::{
    agent::{MicroAgentBuilder, SupportedProvider},
    types::{Agent, AgentError, ToolExecutionContext},
};
use microagents_events::types::AgentEvent;
use microagents_storage::types::AgentStorageChoice;
use std::{str::FromStr, sync::Arc};

use crate::init_env::initialize_environment;

mod init_env;
mod processing;
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

    /// Prompt to run in headless mode
    #[arg(long, short, default_value = None)]
    prompt: Option<String>,

    /// Whether to print some debug/info messages when initializing the environment
    #[arg(long, short, default_value_t = false)]
    verbose: bool,
}

fn storage_choice(storage: Option<String>) -> AgentStorageChoice {
    match storage {
        Some(s) => match s.as_str() {
            "jsonl" => AgentStorageChoice::Jsonl,
            "sqlite" => AgentStorageChoice::Sqlite,
            _ => AgentStorageChoice::Jsonl,
        },
        None => AgentStorageChoice::Jsonl,
    }
}

async fn build_storage(
    storage: Option<String>,
) -> Result<Box<dyn microagents_storage::types::AgentStorage>, AgentError> {
    let st = storage_choice(storage);
    let builder = MicroAgentBuilder::<()>::new(ToolExecutionContext::new(()));
    let builder = builder
        .storage(st)
        .await
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?;
    Ok(builder.storage)
}

async fn build_agent(
    provider: Option<String>,
    model: Option<String>,
    storage: Option<String>,
    skills: Vec<String>,
) -> Result<microagents_core::agent::MicroAgent<()>, AgentError> {
    let st = storage_choice(storage);
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
    base_builder
        .build()
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    initialize_environment(args.verbose).await?;
    if let Some(p) = args.prompt {
        let agent = build_agent(args.provider, args.model, args.storage, args.skill).await?;
        let mut stream = agent
            .run(p, args.session_id)
            .await
            .map_err(|e| e.to_string())?;
        while let Some(ev) = stream.next().await {
            match ev {
                Ok(e) => {
                    let jsonrpc = serde_json::to_string(&e.to_jsonrpc())?;
                    println!("{jsonrpc}");
                }
                Err(err) => {
                    let jsonrpc = serde_json::to_string(&serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "error": { "code": -32603, "message": "Internal Server Error", "data": &err.to_string() }
                    }))?;
                    println!("{jsonrpc}");
                    return Ok(());
                }
            }
        }
        return Ok(());
    }
    if args.verbose {
        println!("Launching TUI...");
    }
    let initial_session = args.session_id.clone();
    let load_history_storage = args.storage.clone();
    tui::run_with_session(
        initial_session,
        move |prompt, session_id| {
            let prov_c = args.provider.clone();
            let model_c = args.model.clone();
            let skill_c = args.skill.clone();
            let storage_c = args.storage.clone();
            async move {
                let agent = build_agent(prov_c, model_c, storage_c, skill_c).await?;
                agent.run(prompt, session_id).await
            }
        },
        move |session_id| {
            let storage_c = load_history_storage.clone();
            async move {
                let storage = build_storage(storage_c).await?;
                storage.get_session(&session_id).await.map_err(|e| {
                    microagents_core::types::AgentError::SessionLoadError(e.to_string())
                })
            }
        },
    )
    .await?;
    Ok(())
}
