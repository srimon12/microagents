use std::{fmt::Debug, path::Path, str::FromStr, sync::Arc};

use thiserror::Error;
use ultrafast_models_sdk::{
    ClientError, Message, UltrafastClient,
    models::{Function, Tool},
};

pub const SKILLS_PATH: &str = ".agents/skills";

#[derive(Debug)]
pub enum SupportedProvider {
    OpenAI,
    Anthropic,
    Mistral,
    Cohere,
    Ollama,
    Azure,
    Bedrock,
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
            "bedrock" => Ok(Self::Bedrock),
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
    #[error("Unable to initialize client")]
    ClientInitFailed(#[from] ClientError),
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
    pub skills: Vec<String>,
    pub providers: Vec<SupportedProvider>,
    pub main_model: String,
    pub fallback_models: Vec<String>,
    client: Option<DebuggableClient>,
}

pub struct MicroAgentBuilder {
    tools: Vec<Tool>,
    skills: Vec<String>,
    providers: Vec<SupportedProvider>,
    main_model: String,
    fallback_models: Vec<String>,
}

impl MicroAgentBuilder {
    pub fn new() -> Self {
        Self {
            tools: vec![],
            skills: vec![],
            providers: vec![],
            main_model: String::new(),
            fallback_models: vec![],
        }
    }

    pub fn add_skill(mut self, skill_name: String) -> Result<Self, MicroAgentBuilderError> {
        let path = Path::new(SKILLS_PATH).join(&skill_name);
        if !path.exists() {
            return Err(MicroAgentBuilderError::SkillNotFound(skill_name));
        }
        self.skills.push(skill_name);
        Ok(self)
    }

    pub fn add_provider(mut self, provider: String) -> Result<Self, MicroAgentBuilderError> {
        let prov = SupportedProvider::from_str(&provider)?;
        self.providers.push(prov);
        Ok(self)
    }

    pub fn main_model(mut self, model: String) -> Self {
        self.main_model = model;
        self
    }

    pub fn add_fallback_model(mut self, model: String) -> Self {
        self.fallback_models.push(model);
        self
    }

    pub fn add_tool(
        mut self,
        name: String,
        description: String,
        parameters: serde_json::Value,
    ) -> Self {
        let tool = Tool {
            tool_type: "function".into(),
            function: Function {
                name,
                description: Some(description),
                parameters,
            },
        };
        self.tools.push(tool);
        self
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
    fn init_client() -> Result<(), MicroAgentBuilderError> {
        Ok(())
    }
}
