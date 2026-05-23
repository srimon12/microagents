use std::{
    collections::HashMap,
    env::{self},
    fmt::{self, Debug},
    str::FromStr,
    sync::Arc,
};

use futures_util::StreamExt;
use thiserror::Error;
use ultrafast_models_sdk::{ChatRequest, Message, UltrafastClient, models::Tool};

use crate::{
    skills::{self, ensure_skill, parse_skill},
    types::{Agent, AgentError, GenerationStream, ToolFunction},
};

pub const SKILLS_PATH: &str = ".agents/skills";
pub const GLOBAL_SKILLS_PATH: &str = "~/.agents/skills";
pub const BASE_SYSTEM_PROMPT: &str = r#"<identity>
You are MicroAgent, an AI agent whose purpose is to
fulfil request coming from a user, employing the tools and skills
available to you and interacting with the environment
you are given
</identity>
<guidelines>
<general>
To carry out a task, follow the main rules of the Zen of Python whenever possible:
- Beautiful is better than ugly.
- Explicit is better than implicit.
- Simple is better than complex.
- Complex is better than complicated.
- Flat is better than nested.
- Readability counts.
- Special cases aren't special enough to break the rules, although practicality beats purity.
- Errors should never pass silently, unless explicitly silenced.
- In the face of ambiguity, refuse the temptation to guess.
- There should be one (and preferably only one) obvious way to do it.
- If the implementation is hard to explain, it's a bad idea.
- If the implementation is easy to explain, it _may_ be a good idea, but **it not necessarily is**.
</general>
<tools_and_skills_usage>
Tools can be invoked by providing their name and an input conforming to their input JSON schema.
Call tools either when requested by the user, or when the description of the tool seems compelling
enough for the task at hand.
You also have a special tool called 'skills'. When you want to access specialized knowledge over a
particular area, you can invoke the skill pertaining to that area by calling the 'skills' tool and
providing the name of the skill to it. The 'skills' tool will return the specific instructions for that
skill. Invoke a skill either when directly prompted by the user to do so, or when the skill's description
seems compelling enough for the task at hand.
</tools_and_skills_usage>
</guidelines>
"#;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum SupportedProvider {
    OpenAI,
    Anthropic,
    Mistral,
    Cohere,
    Ollama,
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
            "groq" => Ok(Self::Groq),
            _ => Err(MicroAgentBuilderError::ProviderNotSupported(s.into())),
        }
    }
}

impl fmt::Display for SupportedProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Anthropic => "anthropic",
            Self::Cohere => "cohere",
            Self::Groq => "groq",
            Self::Mistral => "mistral",
            Self::Ollama => "ollama",
            Self::OpenAI => "openai",
        };
        write!(f, "{}", s)
    }
}

impl Default for SupportedProvider {
    fn default() -> Self {
        SupportedProvider::Anthropic
    }
}

impl SupportedProvider {
    pub fn default_model(&self) -> &'static str {
        match self {
            // Best balance of capability and cost for most workloads
            SupportedProvider::Anthropic => "claude-sonnet-4-6",

            // GPT-5.5 is the current default ChatGPT model as of May 2026
            SupportedProvider::OpenAI => "gpt-5.5",

            // mistral-large-latest always points to the current flagship (2512 as of now)
            // Most reliable for function calling per Mistral's own docs
            SupportedProvider::Mistral => "mistral-large-latest",

            // Command A+ just released (May 2026), their most capable model
            SupportedProvider::Cohere => "command-a-plus-05-2026",

            // llama3.2 is the most widely tested, hardware-friendly default
            SupportedProvider::Ollama => "llama3.2",

            // llama-3.3-70b-versatile is Groq's documented default recommendation
            SupportedProvider::Groq => "llama-3.3-70b-versatile",
        }
    }
}

#[derive(Debug, Error)]
pub enum MicroAgentBuilderError {
    #[error("Skill {0} not found")]
    SkillNotFound(String),
    #[error("Skill parsing error")]
    SkillParsingError(#[from] skills::SkillParsingError),
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
pub struct MicroAgent<Ctx> {
    pub history: Vec<Message>,
    pub tools: HashMap<String, Box<dyn ToolFunction<Ctx>>>,
    pub skills: HashMap<String, String>,
    pub provider: SupportedProvider,
    pub model: String,
    pub system: String,
    client: Option<DebuggableClient>,
}

pub struct MicroAgentBuilder<Ctx> {
    tools: HashMap<String, Box<dyn ToolFunction<Ctx>>>,
    skills: HashMap<String, String>,
    provider: SupportedProvider,
    model: String,
    custom_instructions: String,
}

impl<Ctx> MicroAgentBuilder<Ctx> {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            skills: HashMap::new(),
            provider: SupportedProvider::default(),
            model: String::new(),
            custom_instructions: String::new(),
        }
    }

    pub fn add_skill(mut self, skill_name: String) -> Result<Self, MicroAgentBuilderError> {
        if let Some(skill_path) = ensure_skill(&skill_name) {
            let description = parse_skill(&skill_path)?;
            self.skills.insert(skill_name, description);
            return Ok(self);
        }
        Err(MicroAgentBuilderError::SkillNotFound(skill_name))
    }

    pub fn provider(mut self, provider: String) -> Result<Self, MicroAgentBuilderError> {
        let prov = SupportedProvider::from_str(&provider)?;
        self.provider = prov;
        Ok(self)
    }

    pub fn model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn add_tool(
        mut self,
        tool: Box<dyn ToolFunction<Ctx>>,
    ) -> Result<Self, MicroAgentBuilderError> {
        self.tools.insert(tool.name(), tool);
        Ok(self)
    }

    pub fn custom_instructions(mut self, instructions: String) -> Self {
        self.custom_instructions = instructions;
        self
    }

    fn resolve_model(&self) -> String {
        if self.model.is_empty() {
            return self.provider.default_model().into();
        }
        self.model.clone()
    }

    fn resolve_system(&self) -> String {
        let model = self.resolve_model();
        let mut base = BASE_SYSTEM_PROMPT.to_string();
        base += &format!(
            r#"<model>
You are {} provided by {}
</model>
"#,
            model, self.provider
        );
        if !self.tools.is_empty() {
            base += "\n<tools>";
            for (k, v) in &self.tools {
                base += &format!(
                    "\n<tool>\n<name>{}</name>\n<description>{}</description>\n</tool>",
                    k,
                    v.description()
                )
            }
            base += "\n</tools>"
        }
        if !self.skills.is_empty() {
            base += "\n<skills>";
            for (k, v) in &self.skills {
                base += &format!(
                    "\n<skill>\n<name>{}</name>\n<description>{}</description>\n</skill>",
                    k, v
                );
            }
            base += "\n</skills>";
        }

        base
    }

    pub fn build(self) -> MicroAgent<Ctx> {
        let system = self.resolve_system();
        MicroAgent {
            history: vec![],
            tools: self.tools,
            skills: self.skills,
            model: self.model,
            provider: self.provider,
            client: None,
            system,
        }
    }
}

impl<Ctx> MicroAgent<Ctx> {
    fn init_client(&mut self) -> Result<(), AgentError> {
        if self.client.is_some() {
            return Ok(());
        }
        let mut base_client = UltrafastClient::standalone();
        base_client = match self.provider {
            SupportedProvider::Anthropic => {
                base_client.with_anthropic(env::var("ANTHROPIC_API_KEY")?)
            }
            SupportedProvider::OpenAI => base_client.with_openai(env::var("OPENAI_API_KEY")?),
            SupportedProvider::Groq => base_client.with_groq(env::var("GROQ_API_KEY")?),
            SupportedProvider::Cohere => base_client.with_cohere(env::var("COHERE_API_KEY")?),
            SupportedProvider::Mistral => base_client.with_mistral(env::var("MISTRAL_API_KEY")?),
            SupportedProvider::Ollama => base_client.with_ollama(
                env::var("OLLAMA_BASE_URL").unwrap_or("http://localhost:11434/api".to_string()),
            ),
        };
        let client = base_client.build()?;
        self.client = Some(DebuggableClient(Arc::new(client)));
        Ok(())
    }
}

#[async_trait::async_trait]
impl<Ctx> Agent for MicroAgent<Ctx> {
    async fn generate(mut self) -> Result<GenerationStream, AgentError> {
        self.init_client()?;
        let tools: Vec<Tool> = self.tools.iter().map(|(_, t)| t.to_sdk_tool()).collect();
        if let Some(client) = self.client {
            let stream = client
                .0
                .stream_chat_completion(ChatRequest {
                    model: self.model,
                    messages: self.history,
                    temperature: None,
                    stream: Some(true),
                    max_tokens: None,
                    tools: Some(tools),
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

        Err(AgentError::GenerationError("Client not available".into()))
    }

    async fn call_tool(self, _tool_name: &str, _tool_args: &str) -> Result<Message, AgentError> {
        Err(AgentError::ToolCallError(
            "Tool calling not available".into(),
        ))
    }

    async fn resolve_skill(self, _skill_name: &str) -> Result<Message, AgentError> {
        Err(AgentError::SkillResolutionError)
    }

    async fn run(self, _prompt: String) -> Result<(), AgentError> {
        Err(AgentError::RunError)
    }
}
