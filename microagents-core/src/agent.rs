use std::{
    collections::HashSet,
    env::{self},
    fmt::Debug,
    path::Path,
    str::FromStr,
    sync::Arc,
};

use futures_util::StreamExt;
use thiserror::Error;
use ultrafast_models_sdk::{
    ChatRequest, Message, UltrafastClient,
    models::{Function, Tool},
};

use crate::types::{Agent, AgentError, AgentStream};

pub const SKILLS_PATH: &str = ".agents/skills";

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum SupportedProvider {
    OpenAI,
    Anthropic,
    Mistral,
    Cohere,
    Ollama,
    Azure,
    Groq,
}

impl FromStr for SupportedProvider {
    type Err = MicroAgentBuilderError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openai" => Ok(Self::OpenAI),
            "anthropic" => Ok(Self::Anthropic),
            "mistral" => Ok(Self::Mistral),
            "ollama" => Ok(Self::Ollama),
            "cohere" => Ok(Self::Cohere),
            "azure" => Ok(Self::Azure),
            "groq" => Ok(Self::Groq),
            _ => Err(MicroAgentBuilderError::ProviderNotSupported(s.into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum MicroAgentBuilderError {
    #[error("Skill {0} not found")]
    SkillNotFound(String),
    #[error("Provider {0} not supported")]
    ProviderNotSupported(String),
    #[error("Tool with name {0} already exists")]
    ToolAlreadyDefined(String),
}

pub struct DebuggableClient(pub Arc<UltrafastClient>);

impl Debug for DebuggableClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UltrafastClient")
    }
}

#[derive(Debug)]
pub struct MicroAgent {
    pub history: Vec<Message>,
    pub tools: Vec<Tool>,
    pub skills: HashSet<String>,
    pub providers: HashSet<SupportedProvider>,
    pub main_model: String,
    pub fallback_models: HashSet<String>,
    client: Option<DebuggableClient>,
}

pub struct MicroAgentBuilder {
    tools: Vec<Tool>,
    skills: HashSet<String>,
    providers: HashSet<SupportedProvider>,
    main_model: String,
    fallback_models: HashSet<String>,
    _tool_names: HashSet<String>,
}

impl MicroAgentBuilder {
    pub fn new() -> Self {
        Self {
            tools: vec![],
            skills: HashSet::new(),
            providers: HashSet::new(),
            main_model: String::new(),
            fallback_models: HashSet::new(),
            _tool_names: HashSet::new(),
        }
    }

    pub fn add_skill(mut self, skill_name: String) -> Result<Self, MicroAgentBuilderError> {
        let path = Path::new(SKILLS_PATH).join(&skill_name);
        if !path.exists() {
            return Err(MicroAgentBuilderError::SkillNotFound(skill_name));
        }
        self.skills.insert(skill_name);
        Ok(self)
    }

    pub fn add_provider(mut self, provider: String) -> Result<Self, MicroAgentBuilderError> {
        let prov = SupportedProvider::from_str(&provider)?;
        self.providers.insert(prov);
        Ok(self)
    }

    pub fn main_model(mut self, model: String) -> Self {
        self.main_model = model;
        self
    }

    pub fn add_fallback_model(mut self, model: String) -> Self {
        self.fallback_models.insert(model);
        self
    }

    pub fn add_tool(
        mut self,
        name: String,
        description: String,
        parameters: serde_json::Value,
    ) -> Result<Self, MicroAgentBuilderError> {
        if self._tool_names.contains(&name) {
            return Err(MicroAgentBuilderError::ToolAlreadyDefined(name));
        }
        self._tool_names.insert(name.clone());
        let tool = Tool {
            tool_type: "function".into(),
            function: Function {
                name,
                description: Some(description),
                parameters,
            },
        };
        self.tools.push(tool);
        Ok(self)
    }

    pub fn build(self) -> MicroAgent {
        MicroAgent {
            history: vec![],
            tools: self.tools,
            skills: self.skills,
            main_model: self.main_model,
            fallback_models: self.fallback_models,
            providers: self.providers,
            client: None,
        }
    }
}

impl MicroAgent {
    fn init_client(&mut self) -> Result<(), AgentError> {
        if self.client.is_some() {
            return Ok(());
        }
        let mut base_client = UltrafastClient::standalone();
        for provider in &self.providers {
            base_client = match provider {
                SupportedProvider::Anthropic => {
                    base_client.with_anthropic(env::var("ANTHROPIC_API_KEY")?)
                }
                SupportedProvider::OpenAI => base_client.with_openai(env::var("OPENAI_API_KEY")?),
                SupportedProvider::Azure => base_client.with_azure_openai(
                    env::var("AZURE_OPENAI_API_KEY")?,
                    env::var("AZURE_DEPLOYMENT_NAME")?,
                ),
                SupportedProvider::Groq => base_client.with_groq(env::var("GROQ_API_KEY")?),
                SupportedProvider::Cohere => base_client.with_cohere(env::var("COHERE_API_KEY")?),
                SupportedProvider::Mistral => {
                    base_client.with_mistral(env::var("MISTRAL_API_KEY")?)
                }
                SupportedProvider::Ollama => base_client.with_ollama(
                    env::var("OLLAMA_BASE_URL").unwrap_or("http://localhost:11434/api".to_string()),
                ),
            };
        }
        let client = base_client.build()?;
        self.client = Some(DebuggableClient(Arc::new(client)));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Agent for MicroAgent {
    async fn generate(mut self) -> Result<AgentStream, AgentError> {
        self.init_client()?;
        if let Some(client) = self.client {
            let stream = client
                .0
                .stream_chat_completion(ChatRequest {
                    model: self.main_model,
                    messages: self.history,
                    temperature: None,
                    stream: Some(true),
                    max_tokens: None,
                    tools: Some(self.tools),
                    tool_choice: Some(ultrafast_models_sdk::models::ToolChoice::Auto),
                    top_p: None,
                    frequency_penalty: None,
                    user: None,
                    presence_penalty: None,
                    stop: None,
                })
                .await?;
            let mapped = stream.map(|item| item.map_err(AgentError::ClientInitFailed));
            return Ok(Box::pin(mapped));
        }

        Err(AgentError::GenerationError)
    }

    async fn call_tool(self, _tool_name: &str, _tool_args: &str) -> Result<Message, AgentError> {
        Err(AgentError::ToolCallError)
    }

    async fn resolve_skill(self, _skill_name: &str) -> Result<Message, AgentError> {
        Err(AgentError::SkillResolutionError)
    }

    async fn run(self, _prompt: String) -> Result<(), AgentError> {
        Err(AgentError::RunError)
    }
}
