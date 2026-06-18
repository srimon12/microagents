use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env::{self},
    fmt::{self, Debug},
    fs, io,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use dashmap::DashMap;

use async_stream::stream;
use chrono::Utc;
use futures_util::StreamExt;
use microagents_events::{
    AgentEventAny, AssistantResponseEvent, DeltaType, SessionInitEvent, SessionInitType,
    SessionStopEvent, SkillLoadEvent, StreamDeltaEvent, ToolCallEvent, ToolResultEvent, Usage,
    UserPromptSubmitEvent, types::ToolResult,
};
use microagents_storage::{
    jsonl::JsonlAgentStorage,
    memory::InMemoryAgentStorage,
    sqlite::SqliteAgentStorage,
    types::{AgentStorage, AgentStorageChoice},
};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::{sync::Semaphore, task::JoinSet};
use ultrafast_models_sdk::{
    ChatRequest, CircuitBreakerConfig, Message, ProviderConfig, Role, UltrafastClient,
    cache::{CacheConfig, CacheType},
    models::{Delta, FunctionCall, Tool, ToolCall},
};

use crate::{
    common::{
        JsonResult, call_tool, check_env_var, convert_event_to_message, estimate_tokens,
        load_agents_md, parse_json_fragment,
    },
    skills::{self, ensure_skill, find_skills, parse_skill},
    types::{Agent, AgentError, GenerationStream, RunStream, ToolExecutionContext, ToolFunction},
};

/// Relative path to the project-local skills directory.
pub const SKILLS_PATH: &str = ".agents/skills";
/// Name of the built-in skill-loading tool exposed to the LLM.
pub const SKILLS_TOOL_NAME: &str = "skills";
/// Path alias for the global skills directory (resolved at runtime).
pub const GLOBAL_SKILLS_PATH: &str = "~/.agents/skills";
/// Base system prompt injected into every conversation.
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
- If the implementation is easy to explain, it _may_ be a good idea, but **it is not necessarily**.
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
/// Maximum number of tool calls executed concurrently when
/// `parallel_tool_calls` is enabled.
const MAX_CONCURRENT_TOOL_CALLS: usize = 10;

/// Supported LLM providers.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub enum SupportedProvider {
    #[default]
    OpenAI,
    OpenRouter,
    Ollama,
    Groq,
    OpenAICompatible,
}

pub trait AsProvider {
    fn as_provider(&self) -> Result<SupportedProvider, MicroAgentBuilderError>;
}

impl AsProvider for SupportedProvider {
    fn as_provider(&self) -> Result<SupportedProvider, MicroAgentBuilderError> {
        Ok(self.to_owned())
    }
}

impl FromStr for SupportedProvider {
    type Err = MicroAgentBuilderError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openai" => Ok(Self::OpenAI),
            "openrouter" => Ok(Self::OpenRouter),
            "ollama" => Ok(Self::Ollama),
            "groq" => Ok(Self::Groq),
            "openai-compatible" => Ok(Self::OpenAICompatible),
            _ => Err(MicroAgentBuilderError::ProviderNotSupported(s.into())),
        }
    }
}

impl AsProvider for String {
    fn as_provider(&self) -> Result<SupportedProvider, MicroAgentBuilderError> {
        SupportedProvider::from_str(self)
    }
}

impl fmt::Display for SupportedProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::OpenRouter => "openrouter",
            Self::Groq => "groq",
            Self::Ollama => "ollama",
            Self::OpenAI => "openai",
            Self::OpenAICompatible => "openai-compatible",
        };
        write!(f, "{}", s)
    }
}

impl SupportedProvider {
    /// Return the default model identifier for this provider.
    pub fn default_model(&self) -> Result<&'static str, MicroAgentBuilderError> {
        match self {
            // GPT-5.5 is the current default ChatGPT model as of May 2026
            SupportedProvider::OpenAI => Ok("gpt-5.5"),

            // llama3.2 is the most widely tested, hardware-friendly default
            SupportedProvider::Ollama => Ok("llama3.2"),

            // openai/gpt-oss-120b is Groq's documented default recommendation compatible with prompt caching
            SupportedProvider::Groq => Ok("openai/gpt-oss-120b"),

            // Claude Opus 4.7 by Anthropic is cuttig-edge in the models market
            SupportedProvider::OpenRouter => Ok("anthropic/claude-opus-4.7"),

            // The rest should specify a model
            SupportedProvider::OpenAICompatible => Err(
                MicroAgentBuilderError::ModelNotSpecifiedError("openai-compatible".into()),
            ),
        }
    }
}

/// Errors that can occur while configuring or building a [`MicroAgent`].
#[derive(Debug, Error)]
pub enum MicroAgentBuilderError {
    #[error("Skill {0} not found")]
    SkillNotFound(String),
    #[error("Skill parsing error")]
    SkillParsingError(#[from] skills::SkillLoadingError),
    #[error("Provider {0} not supported")]
    ProviderNotSupported(String),
    #[error("Tool with name {0} already exists")]
    ToolAlreadyDefined(String),
    #[error("Storage could not be loaded: {0}")]
    StorageLoadError(String),
    #[error("Environment variable {0} not found")]
    EnvVarNotFoundError(String),
    #[error("Provider {0} should specify a model")]
    ModelNotSpecifiedError(String),
    #[error(transparent)]
    AgentsMdResolutionError(#[from] io::Error),
}

/// Newtype wrapper so that [`UltrafastClient`] can implement [`Debug`].
pub struct DebuggableClient(pub Arc<UltrafastClient>);

impl Debug for DebuggableClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UltrafastClient")
    }
}

/// A fully-configured agent ready to generate responses or run conversations.
///
/// Created via [`MicroAgentBuilder`]. Holds the conversation history, tool
/// registry, and LLM client configuration.
pub struct MicroAgent<Ctx> {
    pub history: Vec<Message>,
    pub tools: HashMap<String, Arc<dyn ToolFunction<Ctx>>>,
    pub skills: HashMap<String, String>,
    pub provider: SupportedProvider,
    pub model: String,
    pub system: String,
    client: Option<DebuggableClient>,
    /// Per-session clients for OpenRouter so that `x-session-id` is never reused across sessions.
    pub openrouter_clients: DashMap<String, Arc<DebuggableClient>>,
    pub tool_context: Arc<ToolExecutionContext<Ctx>>,
    pub storage: Box<dyn AgentStorage>,
    pub parallel_tool_calls: bool,
}

impl<Ctx: Debug> Debug for MicroAgent<Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MicroAgent")
            .field("history", &self.history)
            .field("tools", &self.tools)
            .field("skills", &self.skills)
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("system", &self.system)
            .field("client", &self.client)
            .field("openrouter_clients", &self.openrouter_clients.len())
            .field("tool_context", &self.tool_context)
            .field("storage", &self.storage)
            .field("parallel_tool_calls", &self.parallel_tool_calls)
            .finish()
    }
}

/// Builder for [`MicroAgent`].
///
/// # Example
/// ```no_run
/// use microagents_core::agent::MicroAgentBuilder;
/// use microagents_core::types::ToolExecutionContext;
///
/// let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
///     .provider("openai".into()).unwrap()
///     .model("gpt-5.5".into())
///     .build()
///     .expect("API key must be set");
/// ```
#[derive(Debug)]
pub struct MicroAgentBuilder<Ctx> {
    tools: HashMap<String, Arc<dyn ToolFunction<Ctx>>>,
    skills: HashMap<String, String>,
    provider: SupportedProvider,
    model: String,
    custom_instructions: String,
    tool_context: Arc<ToolExecutionContext<Ctx>>,
    pub storage: Box<dyn AgentStorage>,
    pub parallel_tool_calls: bool,
}

impl<Ctx: Send + Sync + 'static> MicroAgentBuilder<Ctx> {
    /// Create a new builder with the given tool execution context.
    ///
    /// The `skills` tool is registered automatically.
    pub fn new(tool_context: ToolExecutionContext<Ctx>) -> Self {
        Self {
            tools: HashMap::from([(
                "skills".to_string(),
                Arc::new(SkillsTool) as Arc<dyn ToolFunction<Ctx>>,
            )]),
            skills: HashMap::new(),
            provider: SupportedProvider::default(),
            model: String::new(),
            custom_instructions: String::new(),
            tool_context: Arc::new(tool_context),
            storage: Box::new(InMemoryAgentStorage::default()) as Box<dyn AgentStorage>,
            parallel_tool_calls: false,
        }
    }

    /// Register a single skill by name.
    ///
    /// Searches `.agents/skills/{name}` then `~/.agents/skills/{name}`.
    pub fn add_skill(mut self, skill_name: String) -> Result<Self, MicroAgentBuilderError> {
        if let Some(skill_path) = ensure_skill(&skill_name) {
            let description = parse_skill(&skill_path)?;
            self.skills.insert(skill_name, description);
            return Ok(self);
        }
        Err(MicroAgentBuilderError::SkillNotFound(skill_name))
    }

    /// Auto-discover and register all skills found in the local and global
    /// skills directories.
    pub fn find_skills(mut self) -> Result<Self, MicroAgentBuilderError> {
        let loaded_skills = find_skills()?;
        for (skill, des) in loaded_skills {
            self.skills.insert(skill, des);
        }
        Ok(self)
    }

    /// Set the LLM provider (e.g. `"openai"`, `"groq"`, `"ollama"`).
    pub fn provider(mut self, provider: impl AsProvider) -> Result<Self, MicroAgentBuilderError> {
        let prov = provider.as_provider()?;
        self.provider = prov;
        Ok(self)
    }

    /// Set the model identifier. If empty, the provider's default is used.
    pub fn model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Enable or disable parallel tool execution.
    pub fn parallel_tool_calls(mut self, parallel_tool_calls: bool) -> Self {
        self.parallel_tool_calls = parallel_tool_calls;
        self
    }

    /// Configure the session storage backend.
    pub async fn storage(
        mut self,
        storage: AgentStorageChoice,
    ) -> Result<Self, MicroAgentBuilderError> {
        match storage {
            AgentStorageChoice::Jsonl => self.storage = Box::new(JsonlAgentStorage::default()),
            AgentStorageChoice::Memory => self.storage = Box::new(InMemoryAgentStorage::default()),
            AgentStorageChoice::Sqlite => {
                let store = SqliteAgentStorage::new(None)
                    .await
                    .map_err(|e| MicroAgentBuilderError::StorageLoadError(e.to_string()))?;
                self.storage = Box::new(store);
            }
        }

        Ok(self)
    }

    /// Register a custom tool.
    pub fn add_tool(
        mut self,
        tool: Arc<dyn ToolFunction<Ctx>>,
    ) -> Result<Self, MicroAgentBuilderError> {
        self.tools.insert(tool.name().to_owned(), tool);
        Ok(self)
    }

    /// Append free-form instructions to the system prompt.
    pub fn custom_instructions(mut self, instructions: String) -> Self {
        self.custom_instructions = instructions;
        self
    }

    pub fn load_agents_md(mut self) -> Result<Self, MicroAgentBuilderError> {
        let instructions = load_agents_md()?;
        self.custom_instructions += &instructions;
        Ok(self)
    }

    /// Choose the effective model: user-supplied or provider default.
    fn resolve_model(&self) -> Result<String, MicroAgentBuilderError> {
        if self.model.is_empty() {
            return self.provider.default_model().map(|m| m.to_string());
        }
        Ok(self.model.clone())
    }

    /// Assemble the full system prompt from the base prompt, model info,
    /// registered tools, skills, and any custom instructions.
    fn resolve_system(&self, model: &str) -> String {
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
                    "\n<tool>\n<name>{}</name>\n<description>{}</description>\n<input_schema>{}</input_schema>\n</tool>",
                    k,
                    v.description(),
                    v.input_schema()
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
        if !self.custom_instructions.is_empty() {
            base += &format!(
                "\n<additional_instructions>\n{}\n</additional_instructions>",
                self.custom_instructions
            )
        }

        base
    }

    /// Finalise the builder and return a [`MicroAgent`].
    ///
    /// Fails early if a required API key is missing for the chosen provider.
    #[must_use = "The builder needs to call `build` otherwise it hangs without turning into an actual agent."]
    pub fn build(self) -> Result<MicroAgent<Ctx>, MicroAgentBuilderError> {
        let model = self.resolve_model()?;
        let system = self.resolve_system(&model);
        match self.provider {
            SupportedProvider::Groq => {
                check_env_var("GROQ_API_KEY").map_err(|_| {
                    MicroAgentBuilderError::EnvVarNotFoundError("GROQ_API_KEY".into())
                })?;
            }
            SupportedProvider::OpenAI => {
                check_env_var("OPENAI_API_KEY").map_err(|_| {
                    MicroAgentBuilderError::EnvVarNotFoundError("OPENAI_API_KEY".into())
                })?;
            }
            SupportedProvider::OpenRouter => {
                check_env_var("OPENROUTER_API_KEY").map_err(|_| {
                    MicroAgentBuilderError::EnvVarNotFoundError("OPENROUTER_API_KEY".into())
                })?;
            }
            SupportedProvider::OpenAICompatible => {
                check_env_var("OPENAI_API_KEY").map_err(|_| {
                    MicroAgentBuilderError::EnvVarNotFoundError("OPENAI_API_KEY".into())
                })?;
                check_env_var("OPENAI_BASE_URL").map_err(|_| {
                    MicroAgentBuilderError::EnvVarNotFoundError("OPENAI_BASE_URL".into())
                })?;
            }
            _ => {}
        }
        Ok(MicroAgent {
            history: vec![],
            tools: self.tools,
            skills: self.skills,
            model,
            provider: self.provider,
            client: None,
            openrouter_clients: DashMap::new(),
            system,
            tool_context: self.tool_context,
            storage: self.storage,
            parallel_tool_calls: self.parallel_tool_calls,
        })
    }
}

impl<Ctx> MicroAgent<Ctx> {
    /// Lazily initialise the LLM client.
    ///
    /// For non-OpenRouter providers the client is cached after the first call.
    /// For OpenRouter a separate client is created (and cached) per
    /// `sticky_session_id` so that the `x-session-id` header is never reused
    /// across sessions.
    pub fn init_client(
        &mut self,
        sticky_session_id: &str,
    ) -> Result<Arc<UltrafastClient>, AgentError> {
        if self.provider != SupportedProvider::OpenRouter
            && let Some(c) = self.client.as_ref()
        {
            return Ok(c.0.clone());
        }

        if self.provider == SupportedProvider::OpenRouter
            && let Some(entry) = self.openrouter_clients.get(sticky_session_id)
        {
            return Ok(entry.0.clone());
        }

        let mut base_client = UltrafastClient::standalone();
        base_client = match self.provider {
            SupportedProvider::OpenRouter => base_client.with_provider(
                "openai",
                ProviderConfig {
                    name: "openai".into(),
                    api_key: env::var("OPENROUTER_API_KEY")?,
                    base_url: Some("https://openrouter.ai/api/v1".into()),
                    timeout: Duration::from_secs(300),
                    max_retries: 3,
                    retry_delay: Duration::from_millis(500),
                    rate_limit: None,
                    model_mapping: HashMap::new(),
                    headers: HashMap::from([(
                        "x-session-id".to_string(),
                        sticky_session_id.to_string(),
                    )]),
                    enabled: true,
                    circuit_breaker: Some(CircuitBreakerConfig {
                        failure_threshold: 5,
                        recovery_timeout: Duration::from_secs(30),
                        request_timeout: Duration::from_secs(10),
                        half_open_max_calls: 3,
                    }),
                },
            ),
            SupportedProvider::OpenAI => base_client.with_openai(env::var("OPENAI_API_KEY")?),
            SupportedProvider::Groq => base_client.with_groq(env::var("GROQ_API_KEY")?),
            SupportedProvider::OpenAICompatible => base_client.with_provider(
                "openai",
                ProviderConfig {
                    name: "openai".into(),
                    api_key: env::var("OPENAI_API_KEY")?,
                    base_url: Some(env::var("OPENAI_BASE_URL")?),
                    timeout: Duration::from_secs(300),
                    max_retries: 3,
                    retry_delay: Duration::from_millis(500),
                    rate_limit: None,
                    model_mapping: HashMap::new(),
                    headers: HashMap::new(),
                    enabled: true,
                    circuit_breaker: Some(CircuitBreakerConfig {
                        failure_threshold: 5,
                        recovery_timeout: Duration::from_secs(30),
                        request_timeout: Duration::from_secs(10),
                        half_open_max_calls: 3,
                    }),
                },
            ),
            SupportedProvider::Ollama => base_client.with_provider(
                "openai",
                ProviderConfig {
                    base_url: Some(
                        env::var("OLLAMA_BASE_URL").unwrap_or("http://localhost:11434/v1".into()),
                    ),
                    api_key: "ollama".into(),
                    name: "openai".into(),
                    timeout: Duration::from_secs(300),
                    max_retries: 3,
                    retry_delay: Duration::from_millis(500),
                    rate_limit: None,
                    model_mapping: HashMap::new(),
                    headers: HashMap::new(),
                    enabled: true,
                    circuit_breaker: Some(CircuitBreakerConfig {
                        failure_threshold: 5,
                        recovery_timeout: Duration::from_secs(30),
                        request_timeout: Duration::from_secs(10),
                        half_open_max_calls: 3,
                    }),
                },
            ),
        };
        let client = base_client
            .with_routing_strategy(ultrafast_models_sdk::RoutingStrategy::Single)
            .with_cache(CacheConfig {
                enabled: true,
                ttl: Duration::from_secs(600),
                max_size: 1000,
                cache_type: CacheType::InMemory,
            })
            .build()
            .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?;
        let arcc = Arc::new(client);

        if self.provider == SupportedProvider::OpenRouter {
            self.openrouter_clients.insert(
                sticky_session_id.to_string(),
                Arc::new(DebuggableClient(arcc.clone())),
            );
        } else {
            self.client = Some(DebuggableClient(arcc.clone()));
        }

        Ok(arcc)
    }
}

/// Built-in tool that loads skill instructions at runtime.
#[derive(Debug)]
pub struct SkillsTool;

#[async_trait::async_trait]
impl<Ctx: Send + Sync + 'static> ToolFunction<Ctx> for SkillsTool {
    fn name(&self) -> &'static str {
        SKILLS_TOOL_NAME
    }

    fn description(&self) -> &'static str {
        "Call this tool to load a skill, providing the name of the skill you are invoking"
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
          "type": "object",
          "required": [
            "skill_name"
          ],
          "properties": {
            "skill_name": {
              "type": "string",
              "description": "Name of the skill to load"
            }
          }
        })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &Arc<ToolExecutionContext<Ctx>>,
    ) -> Result<ToolResult, AgentError> {
        let skill_name = input["skill_name"]
            .as_str()
            .ok_or_else(|| AgentError::ToolCallError("missing skill_name".into()))?;
        let skill_path = ensure_skill(skill_name);
        if let Some(p) = skill_path {
            let content = fs::read_to_string(p.join("SKILL.md")).map_err(|e| {
                AgentError::ToolCallError(format!("Skill {skill_name} could not be read: {}", e))
            })?;
            return Ok(ToolResult::Ok(content));
        }
        Ok(ToolResult::Err(format!(
            "Skill {skill_name} could not be found"
        )))
    }
}

#[async_trait::async_trait]
impl<Ctx: Send + Sync + 'static> Agent for MicroAgent<Ctx> {
    /// Generate the next assistant response as a raw token stream.
    ///
    /// The stream yields [`StreamChunk`]s that may contain text deltas or
    /// partial tool calls. Higher-level orchestration (e.g. [`run`]) is
    /// responsible for buffering and acting on tool calls.
    async fn generate(&mut self, sticky_session_id: &str) -> Result<GenerationStream, AgentError> {
        let client = self.init_client(sticky_session_id)?;
        let tools: Vec<Tool> = self.tools.values().map(|t| t.to_sdk_tool()).collect();
        let stream = client
            .stream_chat_completion(ChatRequest {
                model: self.model.clone(),
                messages: self.history.clone(),
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
            .await
            .map_err(|e| AgentError::GenerationError(e.to_string()))?;
        let mapped =
            stream.map(|item| item.map_err(|e| AgentError::GenerationError(e.to_string())));
        return Ok(Box::pin(mapped));
    }

    /// Run a complete conversation turn.
    ///
    /// If `session_id` is [`Some`] the conversation history is restored from
    /// storage; otherwise a new session is created. The returned stream yields
    /// high-level events ([`AgentEventAny`]) including deltas, tool calls,
    /// results, and the final stop event.
    async fn run(
        mut self,
        prompt: String,
        session_id: Option<String>,
    ) -> Result<RunStream, AgentError> {
        let local_tools: HashMap<String, Arc<dyn ToolFunction<Ctx>>> = self.tools.clone();
        let mut input_text = self.system.clone();
        let mut completion_text = String::new();
        let start_processing = Utc::now();
        let s: RunStream = Box::pin(stream! {
            let resolved_sid;
            let messages: Vec<Message> = if let Some(sid) = session_id {
                let ev = AgentEventAny::SessionInit(SessionInitEvent {
                    session_id: sid.clone(),
                    model: self.model.clone(),
                    system: self.system.clone(),
                    provider: self.provider.to_string(),
                    init_type: SessionInitType::Resume,
                    timestamp: Utc::now(),
                });
                yield Ok(ev);

                let events_res = self
                    .storage
                    .get_session(&sid)
                    .await
                    .map_err(|e| AgentError::SessionLoadError(e.to_string()));

                let events = match events_res {
                    Ok(e) => e,
                    Err(e) => {
                        yield Err(AgentError::RunError(format!("Error while getting the session: {}", e)));
                        return;
                    }
                };

                resolved_sid = sid;

                events
                    .iter()
                    .filter_map(|e| convert_event_to_message(e.clone()))
                    .collect()
            } else {
                let sid = uuid::Uuid::new_v4().to_string();
                let sint = SessionInitEvent {
                    session_id: sid.clone(),
                    model: self.model.clone(),
                    system: self.system.clone(),
                    provider: self.provider.to_string(),
                    init_type: SessionInitType::Start,
                    timestamp: Utc::now(),
                };
                resolved_sid = sid;
                let ev = AgentEventAny::SessionInit(sint.clone());
                match self.storage.create_session(sint).await {
                    Ok(_) => {},
                    Err(e) => {
                        yield Err(AgentError::RunError(format!("An error occurred while creating the session in the storage: {}", e)));
                        return;
                    }
                }
                yield Ok(ev);
                vec![]
            };
            self.history = messages;
            self.history.insert(0, Message { role: Role::System, content: self.system.clone(), name: None, tool_calls: None, tool_call_id: None });
            self.history.push(Message {
                role: Role::User,
                content: prompt.to_owned(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
            input_text += &prompt;
            let turn_id = uuid::Uuid::new_v4().to_string();
            let user_prompt_submit = AgentEventAny::UserPromptSubmit(UserPromptSubmitEvent {
                session_id: resolved_sid.clone(),
                turn_id: turn_id.clone(),
                prompt,
                timestamp: Utc::now(),
            });
            match self.storage.update_session(user_prompt_submit.clone()).await {
                Ok(_) => {},
                Err(e) => {
                    yield Err(AgentError::RunError(format!("An error occurred while updating the session in the storage: {}", e)));
                    return;
                }
            };
            yield Ok(user_prompt_submit);

            loop {
                let mut generation = match self.generate(&resolved_sid.clone()).await {
                    Ok(g) => g,
                    Err(e) => {
                        yield Err(AgentError::RunError(format!("An error occurred while starting the generation stream: {}", e)));
                        return;
                    }
                };
                let mut text = String::new();
                let mut tool_messages: Vec<Message> = vec![];
                let mut tool_calls: BTreeMap<u32, (String, String, String)> = BTreeMap::new();
                while let Some(g) = generation.next().await {
                    match g {
                        Ok(chunk) => {
                            let mut deltas: Vec<(u32, Delta)> = vec![];
                            for choice in chunk.choices {
                                deltas.push((choice.index, choice.delta));
                            }
                            deltas.sort_by_key(|a| a.0);
                            for (_, delta) in deltas {
                                if let Some(c) = delta.content {
                                    text += &c;
                                    completion_text += &c;
                                    let ev = AgentEventAny::StreamDelta(StreamDeltaEvent {
                                        session_id: resolved_sid.clone(),
                                        turn_id: turn_id.clone(),
                                        delta: c,
                                        delta_type: DeltaType::Text,
                                        timestamp: Utc::now(),
                                    });
                                    match self.storage.update_session(ev.clone()).await {
                                        Ok(_) => {},
                                        Err(e) => {
                                            yield Err(AgentError::RunError(format!("An error occurred while updating the session in the storage: {}", e)));
                                            return;
                                        }
                                    }
                                    yield Ok(ev);
                                }
                                if let Some(tcs) = delta.tool_calls {
                                    for tc in tcs {
                                        if let Some(func) = tc.function {
                                            if let Some(tid) = tc.id && let Some(name) = func.name {
                                                // First chunk with id and name: initialize entry
                                                tool_calls.entry(tc.index).or_insert((tid, name, String::new()));
                                            }
                                            // Accumulate arguments regardless
                                            if let Some(args) = func.arguments {
                                                tool_calls.entry(tc.index).and_modify(|v| v.2 += &args);
                                                completion_text += &args;
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            let latency = (Utc::now() - start_processing).num_milliseconds();
                            let stop_ev = AgentEventAny::SessionStop(SessionStopEvent { session_id: resolved_sid.clone(), success: false, result: None, error: Some(e.to_string()), timestamp: Utc::now(), usage: Usage {
                                latency,
                                ..Default::default()
                            }});
                            match self.storage.update_session(stop_ev.clone()).await {
                                Ok(_) => {},
                                Err(e) => {
                                    yield Err(AgentError::RunError(format!("An error occurred while starting the generation stream: {}", e)));
                                    return;
                                }
                            }
                            yield Ok(stop_ev);
                            return;
                        }
                    }
                }

                if tool_calls.is_empty() {
                    let latency = (Utc::now() - start_processing).num_milliseconds();
                    let input_tokens = estimate_tokens(&input_text).unwrap_or_default();
                    let output_tokens = estimate_tokens(&completion_text).unwrap_or_default();
                    let ev = AgentEventAny::AssistantResponse(AssistantResponseEvent {
                        session_id: resolved_sid.clone(),
                        turn_id: turn_id.clone(),
                        full_text: text.clone(),
                        tool_calls: None,
                        timestamp: Utc::now(),
                    });
                    let stop_ev = AgentEventAny::SessionStop(SessionStopEvent {
                        session_id: resolved_sid.clone(),
                        success: true,
                        result: Some(text),
                        error: None,
                        timestamp: Utc::now(),
                        usage: Usage {
                            latency,
                            output_chars: completion_text.len(),
                            input_chars: input_text.len(),
                            estimated_output_tokens: output_tokens,
                            estimated_input_tokens: input_tokens,
                        }
                    });
                    match self.storage.update_session(ev.clone()).await {
                        Ok(_) => {},
                        Err(e) => {
                            yield Err(AgentError::RunError(format!("An error occurred while starting the generation stream: {}", e)));
                            return;
                        }
                    }
                    match self.storage.update_session(stop_ev.clone()).await {
                        Ok(_) => {},
                        Err(e) => {
                            yield Err(AgentError::RunError(format!("An error occurred while starting the generation stream: {}", e)));
                            return;
                        }
                    }
                    yield Ok(ev);
                    yield Ok(stop_ev);
                    return;
                }

                let mut to_pop = HashSet::new();
                let mut to_call = JoinSet::new();
                let tool_ctx = self.tool_context.clone();
                let concurrency = if !self.parallel_tool_calls {
                    1
                } else {
                    MAX_CONCURRENT_TOOL_CALLS
                };
                let semaphore = Arc::new(Semaphore::new(concurrency));
                for (tid, name, args) in tool_calls.values() {
                    match parse_json_fragment(args) {
                        JsonResult::Valid(v) => {
                            let tool = local_tools.get(name);
                            if let Some(t) = tool {
                                let tool_name = name.clone();
                                let tc_ev = if tool_name != SKILLS_TOOL_NAME {
                                    AgentEventAny::ToolCall(ToolCallEvent {
                                        session_id: resolved_sid.clone(),
                                        turn_id: turn_id.clone(),
                                        name: tool_name,
                                        input: v.clone(),
                                        timestamp: Utc::now(),
                                    })
                                } else {
                                    AgentEventAny::SkillLoad(SkillLoadEvent {
                                        session_id: resolved_sid.clone(),
                                        turn_id: turn_id.clone(),
                                        skill_name: v["skill_name"].as_str().unwrap_or_default().to_string(),
                                        timestamp: Utc::now(),
                                    })
                                };
                                match self.storage.update_session(tc_ev.clone()).await {
                                    Ok(_) => {},
                                    Err(e) => {
                                        yield Err(AgentError::RunError(format!("An error occurred while updating the session in the storage: {}", e)));
                                        return;
                                    }
                                }
                                yield Ok(tc_ev);
                                let permit_res = semaphore.clone().acquire_owned().await;
                                let permit = match permit_res {
                                    Ok(p) => p,
                                    Err(e) => {
                                        yield Err(AgentError::RunError(format!("Error while acquiring semaphore: {}", e)));
                                        return;
                                    }
                                };
                                let t = t.clone();
                                let tool_call_id = tid.clone();
                                let ctx = tool_ctx.clone();
                                to_call.spawn(async move {
                                    let _permit = permit;
                                    let result = call_tool(t, v, ctx).await;
                                    match result {
                                        Ok(r) => Ok((tool_call_id, r)),
                                        Err(e) => Err(e)
                                    }
                                });
                            }
                        },
                        JsonResult::Incomplete => {},
                        JsonResult::Malformed => {
                            to_pop.insert(tid.clone());
                        }
                    }
                }
                while let Some(res) = to_call.join_next().await {
                    match res {
                        Ok(Ok((tid, tool_result))) => {
                            let ev = AgentEventAny::ToolResult(ToolResultEvent {
                                session_id: resolved_sid.clone(),
                                turn_id: turn_id.clone(),
                                result: tool_result.clone(),
                                tool_call_id: tid.clone(),
                                timestamp: Utc::now(),
                            });
                            match self.storage.update_session(ev.clone()).await {
                                Ok(_) => {},
                                Err(e) => {
                                    yield Err(AgentError::RunError(format!("An error occurred while updating the session in the storage: {}", e)));
                                    return;
                                }
                            }
                            yield Ok(ev);
                            let content = match tool_result {
                                ToolResult::Ok(r) => {
                                    format!("Tool succeeded: {r}")
                                },
                                ToolResult::Err(r) => {
                                    format!("Tool failed: {r}")
                                },
                                _ => unreachable!("ToolResult should not reach this branch")
                            };
                            input_text += &content;
                            tool_messages.push(Message { role: Role::Tool, content, name: None, tool_calls: None, tool_call_id: Some(tid) });
                        }
                        Ok(Err(e)) => {
                            yield Err(AgentError::RunError(format!("Tool call failed: {}", e)));
                        }
                        Err(e) => {
                            yield Err(AgentError::RunError(format!("Task join failed: {}", e)));
                        }
                    }
                }

                self.history.push(Message {
                    role: Role::Assistant,
                    content: std::mem::take(&mut text),
                    name: None,
                    tool_calls: Some(tool_calls.iter().
                        filter(|(_, (tid, _, _))| !to_pop.contains(tid))
                        .map(|(_, (tid, name, args))| ToolCall {
                        call_type: "function".into(),
                        id: tid.clone(),
                        function: FunctionCall {
                            name: name.clone(),
                            arguments: args.clone(),
                        }
                    }).collect()),
                    tool_call_id: None,
                });
                self.history.extend(tool_messages);
            }
        });
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Agent, AgentError, GenerationStream, RunStream, ToolExecutionContext, ToolFunction,
    };
    use async_stream::stream;
    use futures_util::StreamExt;
    use microagents_events::types::ToolResult;
    use serde_json::Value;
    use std::sync::Arc;

    // ------------------------------------------------------------------
    // DummyAgent – a mock implementation of the Agent trait
    // ------------------------------------------------------------------

    #[derive(Debug)]
    struct DummyAgent {
        pub generate_called: bool,
        pub run_called: bool,
        pub last_prompt: Option<String>,
        pub last_session_id: Option<String>,
    }

    impl DummyAgent {
        fn new() -> Self {
            Self {
                generate_called: false,
                run_called: false,
                last_prompt: None,
                last_session_id: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl Agent for DummyAgent {
        async fn generate(
            &mut self,
            _sticky_session_id: &str,
        ) -> Result<GenerationStream, AgentError> {
            self.generate_called = true;
            let s = stream! {
                yield Ok(ultrafast_models_sdk::models::StreamChunk {
                    id: "1".into(),
                    object: "chat.completion.chunk".into(),
                    created: 0,
                    model: "dummy".into(),
                    choices: vec![],
                });
            };
            Ok(Box::pin(s))
        }

        async fn run(
            mut self,
            prompt: String,
            session_id: Option<String>,
        ) -> Result<RunStream, AgentError> {
            self.run_called = true;
            self.last_prompt = Some(prompt.clone());
            self.last_session_id = session_id.clone();
            let s = stream! {
                yield Ok(AgentEventAny::UserPromptSubmit(UserPromptSubmitEvent {
                    session_id: session_id.unwrap_or_else(|| "new".into()),
                    turn_id: "t1".into(),
                    prompt,
                    timestamp: Utc::now(),
                }));
            };
            Ok(Box::pin(s))
        }
    }

    // ------------------------------------------------------------------
    // A simple dummy tool for builder tests
    // ------------------------------------------------------------------

    #[derive(Debug)]
    struct DummyTool;

    #[async_trait::async_trait]
    impl ToolFunction<()> for DummyTool {
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn description(&self) -> &'static str {
            "A dummy tool"
        }
        fn input_schema(&self) -> Value {
            json!({"type": "object"})
        }
        async fn execute(
            &self,
            _input: Value,
            _ctx: &Arc<ToolExecutionContext<()>>,
        ) -> Result<ToolResult, AgentError> {
            Ok(ToolResult::Ok("done".into()))
        }
    }

    // ------------------------------------------------------------------
    // Builder default tests
    // ------------------------------------------------------------------

    #[test]
    fn test_builder_default_provider_is_openai() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()));
        assert_eq!(builder.provider, SupportedProvider::OpenAI);
    }

    #[test]
    fn test_builder_default_model_is_empty() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()));
        assert!(builder.model.is_empty());
    }

    #[test]
    fn test_builder_default_skills_is_empty() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()));
        assert!(builder.skills.is_empty());
    }

    #[test]
    fn test_builder_default_tools_contains_skills_tool() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()));
        assert!(builder.tools.contains_key("skills"));
        assert_eq!(builder.tools.len(), 1);
    }

    #[test]
    fn test_builder_default_parallel_tool_calls_is_false() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()));
        assert!(!builder.parallel_tool_calls);
    }

    // ------------------------------------------------------------------
    // Builder pattern tests
    // ------------------------------------------------------------------

    #[test]
    fn test_builder_provider_sets_provider() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("groq".to_string())
            .unwrap();
        assert_eq!(builder.provider, SupportedProvider::Groq);
    }

    #[test]
    fn test_builder_provider_invalid_returns_error() {
        let result =
            MicroAgentBuilder::new(ToolExecutionContext::new(())).provider("unknown".to_string());
        assert!(result.is_err());
        assert!(
            matches!(
                result.unwrap_err(),
                MicroAgentBuilderError::ProviderNotSupported(_)
            ),
            "expected ProviderNotSupported error"
        );
    }

    #[test]
    fn test_builder_with_supported_provider_as_enum() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider(SupportedProvider::Ollama)
            .unwrap();
        assert_eq!(builder.provider, SupportedProvider::Ollama);
    }

    #[test]
    fn test_builder_model_sets_model() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(())).model("gpt-5.5".into());
        assert_eq!(builder.model, "gpt-5.5");
    }

    #[test]
    fn test_builder_parallel_tool_calls_sets_flag() {
        let builder =
            MicroAgentBuilder::new(ToolExecutionContext::new(())).parallel_tool_calls(true);
        assert!(builder.parallel_tool_calls);
    }

    #[test]
    fn test_builder_custom_instructions_sets_instructions() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .custom_instructions("Be concise".into());
        assert_eq!(builder.custom_instructions, "Be concise");
    }

    #[test]
    fn test_builder_add_tool_increments_tools() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .add_tool(Arc::new(DummyTool))
            .unwrap();
        assert_eq!(builder.tools.len(), 2);
        assert!(builder.tools.contains_key("dummy"));
    }

    #[tokio::test]
    async fn test_builder_storage_sets_jsonl() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .storage(AgentStorageChoice::Jsonl)
            .await
            .unwrap();
        // We cannot directly inspect the dyn type, but building should succeed
        let _agent = builder.build().expect("Should be able to build the agent");
    }

    #[tokio::test]
    async fn test_builder_storage_sets_memory() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .storage(AgentStorageChoice::Memory)
            .await
            .unwrap();
        let _agent = builder.build().expect("Should be able to build the agent");
    }

    #[tokio::test]
    async fn test_builder_storage_sets_sqlite() {
        let builder = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .storage(AgentStorageChoice::Sqlite)
            .await
            .unwrap();
        let _agent = builder.build().expect("Should be able to build the agent");
    }

    // ------------------------------------------------------------------
    // Build / resolve tests
    // ------------------------------------------------------------------

    #[test]
    fn test_build_sets_empty_history() {
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("ollama".to_string())
            .unwrap()
            .build()
            .expect("Should be able to build the agent");
        assert!(agent.history.is_empty());
    }

    #[test]
    fn test_build_sets_tools_on_agent() {
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("ollama".to_string())
            .unwrap()
            .add_tool(Arc::new(DummyTool))
            .unwrap()
            .build()
            .expect("Should be able to build the agent");
        assert_eq!(agent.tools.len(), 2);
    }

    #[test]
    fn test_build_sets_provider_on_agent() {
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("ollama".to_string())
            .unwrap()
            .build()
            .expect("Should be able to build the agent");
        assert_eq!(agent.provider, SupportedProvider::Ollama);
    }

    #[test]
    fn test_build_sets_model_on_agent() {
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("ollama".to_string())
            .unwrap()
            .model("llama3.2".into())
            .build()
            .expect("Should be able to build the agent");
        assert_eq!(agent.model, "llama3.2");
    }

    #[test]
    fn test_build_sets_parallel_tool_calls_on_agent() {
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("ollama".to_string())
            .unwrap()
            .parallel_tool_calls(true)
            .build()
            .expect("Should be able to build the agent");
        assert!(agent.parallel_tool_calls);
    }

    #[test]
    fn test_build_system_prompt_contains_base() {
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("ollama".to_string())
            .unwrap()
            .build()
            .expect("Should be able to build the agent");
        assert!(agent.system.contains("You are MicroAgent"));
    }

    #[test]
    fn test_build_system_prompt_contains_tools() {
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("ollama".to_string())
            .unwrap()
            .add_tool(Arc::new(DummyTool))
            .unwrap()
            .build()
            .expect("Should be able to build the agent");
        assert!(agent.system.contains("<tools>"));
        assert!(agent.system.contains("<name>dummy</name>"));
    }

    #[test]
    fn test_build_system_prompt_contains_default_model_when_model_empty() {
        let original_value = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "test");
        }
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .build()
            .expect("Should be able to build the agent");
        // Default provider is OpenAI -> default model is gpt-5.5
        assert!(agent.system.contains("gpt-5.5"));
        unsafe {
            std::env::set_var("OPENAI_API_KEY", original_value);
        }
    }

    #[test]
    fn test_build_system_prompt_contains_custom_model_when_set() {
        let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("ollama".to_string())
            .unwrap()
            .model("custom-model".into())
            .build()
            .expect("Should be able to build the agent");
        assert!(agent.system.contains("custom-model"));
        assert!(!agent.system.contains("llama-3.2"));
    }

    #[test]
    fn test_agent_fails_to_build_if_not_api_key() {
        let result = MicroAgentBuilder::new(ToolExecutionContext::new(()))
            .provider("groq".to_string())
            .unwrap()
            .build();
        assert!(result.is_err_and(|e| matches!(e, MicroAgentBuilderError::EnvVarNotFoundError(_))));
    }

    // ------------------------------------------------------------------
    // SupportedProvider tests
    // ------------------------------------------------------------------

    #[test]
    fn test_supported_provider_from_str_valid() {
        assert_eq!(
            SupportedProvider::from_str("openai").unwrap(),
            SupportedProvider::OpenAI
        );
        assert_eq!(
            SupportedProvider::from_str("openrouter").unwrap(),
            SupportedProvider::OpenRouter
        );
        assert_eq!(
            SupportedProvider::from_str("ollama").unwrap(),
            SupportedProvider::Ollama
        );
        assert_eq!(
            SupportedProvider::from_str("groq").unwrap(),
            SupportedProvider::Groq
        );
        assert_eq!(
            SupportedProvider::from_str("openai-compatible").unwrap(),
            SupportedProvider::OpenAICompatible
        );
    }

    #[test]
    fn test_supported_provider_from_str_invalid() {
        assert!(SupportedProvider::from_str("azure").is_err());
    }

    #[test]
    fn test_supported_provider_display() {
        assert_eq!(SupportedProvider::OpenAI.to_string(), "openai");
        assert_eq!(SupportedProvider::OpenRouter.to_string(), "openrouter");
        assert_eq!(SupportedProvider::Ollama.to_string(), "ollama");
        assert_eq!(SupportedProvider::Groq.to_string(), "groq");
        assert_eq!(
            SupportedProvider::OpenAICompatible.to_string(),
            "openai-compatible"
        );
    }

    #[test]
    fn test_supported_provider_default_model() {
        assert_eq!(
            SupportedProvider::OpenAI.default_model().unwrap(),
            "gpt-5.5"
        );
        assert_eq!(
            SupportedProvider::Ollama.default_model().unwrap(),
            "llama3.2"
        );
        assert_eq!(
            SupportedProvider::Groq.default_model().unwrap(),
            "openai/gpt-oss-120b"
        );
        assert_eq!(
            SupportedProvider::OpenRouter.default_model().unwrap(),
            "anthropic/claude-opus-4.7"
        );
        assert!(
            SupportedProvider::OpenAICompatible
                .default_model()
                .is_err_and(|e| matches!(e, MicroAgentBuilderError::ModelNotSpecifiedError(_)))
        )
    }

    #[test]
    fn test_supported_provider_default_is_openai() {
        let provider: SupportedProvider = Default::default();
        assert_eq!(provider, SupportedProvider::OpenAI);
    }

    // ------------------------------------------------------------------
    // DummyAgent mock tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_dummy_agent_generate_sets_flag() {
        let mut agent = DummyAgent::new();
        assert!(!agent.generate_called);
        let _ = agent.generate("hello").await;
        assert!(agent.generate_called);
    }

    #[tokio::test]
    async fn test_dummy_agent_generate_returns_stream() {
        let mut agent = DummyAgent::new();
        let mut stream = agent.generate("hello").await.unwrap();
        let item = stream.next().await;
        assert!(item.is_some());
    }

    #[tokio::test]
    async fn test_dummy_agent_run_streams_prompt() {
        let agent = DummyAgent::new();
        let mut stream = agent
            .run("hello".into(), Some("sid-123".into()))
            .await
            .unwrap();
        // We consumed self in run, so we can't check the fields directly.
        // Instead we verify via the yielded event.
        let item = stream.next().await.unwrap().unwrap();
        match item {
            AgentEventAny::UserPromptSubmit(ev) => {
                assert_eq!(ev.prompt, "hello");
                assert_eq!(ev.session_id, "sid-123");
            }
            _ => panic!("expected UserPromptSubmit"),
        }
    }

    #[tokio::test]
    async fn test_dummy_agent_run_with_none_session_id() {
        let agent = DummyAgent::new();
        let mut stream = agent.run("test".into(), None).await.unwrap();
        let item = stream.next().await.unwrap().unwrap();
        match item {
            AgentEventAny::UserPromptSubmit(ev) => {
                assert_eq!(ev.session_id, "new");
            }
            _ => panic!("expected UserPromptSubmit"),
        }
    }

    #[tokio::test]
    async fn test_dummy_agent_run_stream_yields_single_event() {
        let agent = DummyAgent::new();
        let mut stream = agent.run("prompt".into(), None).await.unwrap();
        let first = stream.next().await;
        assert!(first.is_some());
        let second = stream.next().await;
        assert!(second.is_none());
    }
}
