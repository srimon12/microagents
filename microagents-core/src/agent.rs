use std::{
    collections::{HashMap, HashSet},
    env::{self},
    fmt::{self, Debug},
    fs,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use async_stream::stream;
use futures_util::StreamExt;
use microagents_events::{
    AgentEventAny, AssistantResponseEvent, DeltaType, SessionInitEvent, SessionInitType,
    SessionStopEvent, SkillLoadEvent, StreamDeltaEvent, ToolCallEvent, ToolResultEvent,
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
    common::{JsonResult, call_tool, convert_event_to_message, is_valid_json},
    skills::{self, ensure_skill, find_skills, parse_skill},
    types::{Agent, AgentError, GenerationStream, RunStream, ToolExecutionContext, ToolFunction},
};

pub const SKILLS_PATH: &str = ".agents/skills";
pub const SKILLS_TOOL_NAME: &str = "skills";
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
    OpenRouter,
    Ollama,
    Groq,
}

impl FromStr for SupportedProvider {
    type Err = MicroAgentBuilderError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openai" => Ok(Self::OpenAI),
            "openrouter" => Ok(Self::OpenRouter),
            "ollama" => Ok(Self::Ollama),
            "groq" => Ok(Self::Groq),
            _ => Err(MicroAgentBuilderError::ProviderNotSupported(s.into())),
        }
    }
}

impl fmt::Display for SupportedProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::OpenRouter => "openrouter",
            Self::Groq => "groq",
            Self::Ollama => "ollama",
            Self::OpenAI => "openai",
        };
        write!(f, "{}", s)
    }
}

impl Default for SupportedProvider {
    fn default() -> Self {
        SupportedProvider::OpenAI
    }
}

impl SupportedProvider {
    pub fn default_model(&self) -> &'static str {
        match self {
            // GPT-5.5 is the current default ChatGPT model as of May 2026
            SupportedProvider::OpenAI => "gpt-5.5",

            // llama3.2 is the most widely tested, hardware-friendly default
            SupportedProvider::Ollama => "llama3.2",

            // llama-3.3-70b-versatile is Groq's documented default recommendation
            SupportedProvider::Groq => "llama-3.3-70b-versatile",

            // Claude Opus 4.7 by Anthropic is cuttig-edge in the models market
            SupportedProvider::OpenRouter => "anthropic/claude-opus-4.7",
        }
    }
}

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
    pub tools: HashMap<String, Arc<dyn ToolFunction<Ctx>>>,
    pub skills: HashMap<String, String>,
    pub provider: SupportedProvider,
    pub model: String,
    pub system: String,
    client: Option<DebuggableClient>,
    pub tool_context: Arc<ToolExecutionContext<Ctx>>,
    pub storage: Box<dyn AgentStorage>,
    pub parallel_tool_calls: bool,
}

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

    pub fn add_skill(mut self, skill_name: String) -> Result<Self, MicroAgentBuilderError> {
        if let Some(skill_path) = ensure_skill(&skill_name) {
            let description = parse_skill(&skill_path)?;
            self.skills.insert(skill_name, description);
            return Ok(self);
        }
        Err(MicroAgentBuilderError::SkillNotFound(skill_name))
    }

    pub fn find_skills(mut self) -> Result<Self, MicroAgentBuilderError> {
        let loaded_skills = find_skills()?;
        for (skill, des) in loaded_skills {
            self.skills.insert(skill, des);
        }
        Ok(self)
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

    pub fn parallel_tool_calls(mut self, parallel_tool_calls: bool) -> Self {
        self.parallel_tool_calls = parallel_tool_calls;
        self
    }

    pub async fn storage(
        mut self,
        storage: AgentStorageChoice,
    ) -> Result<Self, MicroAgentBuilderError> {
        match storage {
            AgentStorageChoice::Jsonl => self.storage = Box::new(JsonlAgentStorage {}),
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

    pub fn add_tool(
        mut self,
        tool: Arc<dyn ToolFunction<Ctx>>,
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
            tool_context: self.tool_context,
            storage: self.storage,
            parallel_tool_calls: self.parallel_tool_calls,
        }
    }
}

impl<Ctx> MicroAgent<Ctx> {
    fn init_client(&mut self) -> Result<Arc<UltrafastClient>, AgentError> {
        if let Some(c) = self.client.as_ref() {
            return Ok(c.0.clone());
        }
        let mut base_client = UltrafastClient::standalone();
        base_client = match self.provider {
            SupportedProvider::OpenRouter => {
                base_client.with_openrouter(env::var("OPENROUTER_API_KEY")?)
            }
            SupportedProvider::OpenAI => base_client.with_openai(env::var("OPENAI_API_KEY")?),
            SupportedProvider::Groq => base_client.with_groq(env::var("GROQ_API_KEY")?),
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
        self.client = Some(DebuggableClient(arcc.clone()));
        Ok(arcc)
    }
}

#[derive(Debug)]
pub struct SkillsTool;

#[async_trait::async_trait]
impl<Ctx: Send + Sync + 'static> ToolFunction<Ctx> for SkillsTool {
    fn name(&self) -> String {
        SKILLS_TOOL_NAME.into()
    }

    fn description(&self) -> String {
        "Call this tool to load a skill, providing the name of the skill you are invoking".into()
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
        let skill_name = input["skill_name"].as_str().unwrap();
        let skill_path = ensure_skill(skill_name);
        if let Some(p) = skill_path {
            let content = fs::read_to_string(&p.join("SKILL.md")).map_err(|e| {
                AgentError::ToolCallError(format!(
                    "Skill {skill_name} could not be read: {}",
                    e.to_string()
                ))
            })?;
            return Ok(ToolResult::Ok(content));
        }
        Ok(ToolResult::Err(
            "Skill {skill_name} could not found".to_string(),
        ))
    }
}

#[async_trait::async_trait]
impl<Ctx: Send + Sync + 'static> Agent for MicroAgent<Ctx> {
    async fn generate(&mut self) -> Result<GenerationStream, AgentError> {
        let client = self.init_client()?;
        let tools: Vec<Tool> = self.tools.iter().map(|(_, t)| t.to_sdk_tool()).collect();
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

    async fn run(
        mut self,
        prompt: String,
        session_id: Option<String>,
    ) -> Result<RunStream, AgentError> {
        let local_tools: HashMap<String, Arc<dyn ToolFunction<Ctx>>> = self.tools.clone();
        let s: RunStream = Box::pin(stream! {
            let resolved_sid;
            let messages: Vec<Message> = if let Some(sid) = session_id {
                let ev = AgentEventAny::SessionInit(SessionInitEvent {
                    session_id: sid.clone(),
                    model: self.model.clone(),
                    system: self.system.clone(),
                    provider: self.provider.to_string(),
                    init_type: SessionInitType::Resume,
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
                        yield Err(AgentError::RunError(format!("Error while getting the session: {}", e.to_string())));
                        return;
                    }
                };

                resolved_sid = sid;

                events
                    .iter()
                    .map(|e| convert_event_to_message(e.clone()))
                    .filter(|m| m.is_some())
                    .map(|m| m.unwrap())
                    .collect()
            } else {
                let sid = uuid::Uuid::new_v4().to_string();
                let sint = SessionInitEvent {
                    session_id: sid.clone(),
                    model: self.model.clone(),
                    system: self.system.clone(),
                    provider: self.provider.to_string(),
                    init_type: SessionInitType::Start,
                };
                resolved_sid = sid;
                let ev = AgentEventAny::SessionInit(sint.clone());
                match self.storage.create_session(sint).await {
                    Ok(_) => {},
                    Err(e) => {
                        yield Err(AgentError::RunError(format!("An error occurred while creating the session in the storage: {}", e.to_string())));
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
            let turn_id = uuid::Uuid::new_v4().to_string();
            let user_prompt_submit = AgentEventAny::UserPromptSubmit(UserPromptSubmitEvent {
                session_id: resolved_sid.clone(),
                turn_id: turn_id.clone(),
                prompt,
            });
            match self.storage.update_session(user_prompt_submit.clone()).await {
                Ok(_) => {},
                Err(e) => {
                    yield Err(AgentError::RunError(format!("An error occurred while updating the session in the storage: {}", e.to_string())));
                    return;
                }
            };
            yield Ok(user_prompt_submit);

            loop {
                let mut generation = match self.generate().await {
                    Ok(g) => g,
                    Err(e) => {
                        yield Err(AgentError::RunError(format!("An error occurred while starting the generation stream: {}", e.to_string())));
                        return;
                    }
                };
                let mut text = String::new();
                let mut tool_messages: Vec<Message> = vec![];
                let mut tool_calls: HashMap<u32, (String, String, String)> = HashMap::new();
                while let Some(g) = generation.next().await {
                    match g {
                        Ok(chunk) => {
                            let mut deltas: Vec<(u32, Delta)> = vec![];
                            for choice in chunk.choices {
                                deltas.push((choice.index, choice.delta));
                            }
                            deltas.sort_by(|a, b| a.0.cmp(&b.0));
                            for (_, delta) in deltas {
                                if let Some(c) = delta.content {
                                    text += &c;
                                    let ev = AgentEventAny::StreamDelta(StreamDeltaEvent {
                                        session_id: resolved_sid.clone(),
                                        turn_id: turn_id.clone(),
                                        delta: c,
                                        delta_type: DeltaType::Text,
                                    });
                                    match self.storage.update_session(ev.clone()).await {
                                        Ok(_) => {},
                                        Err(e) => {
                                            yield Err(AgentError::RunError(format!("An error occurred while updating the session in the storage: {}", e.to_string())));
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
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            let stop_ev = AgentEventAny::SessionStop(SessionStopEvent { session_id: resolved_sid.clone(), success: false, result: None, error: Some(e.to_string()) });
                            match self.storage.update_session(stop_ev.clone()).await {
                                Ok(_) => {},
                                Err(e) => {
                                    yield Err(AgentError::RunError(format!("An error occurred while starting the generation stream: {}", e.to_string())));
                                    return;
                                }
                            }
                            yield Ok(stop_ev);
                            return;
                        }
                    }
                }

                if tool_calls.is_empty() {
                    let ev = AgentEventAny::AssistantResponse(AssistantResponseEvent {
                        session_id: resolved_sid.clone(),
                        turn_id: turn_id.clone(),
                        full_text: text.clone(),
                        tool_calls: None,
                    });
                    let stop_ev = AgentEventAny::SessionStop(SessionStopEvent {
                        session_id: resolved_sid.clone(),
                        success: true,
                        result: Some(text),
                        error: None,
                    });
                    match self.storage.update_session(ev.clone()).await {
                        Ok(_) => {},
                        Err(e) => {
                            yield Err(AgentError::RunError(format!("An error occurred while starting the generation stream: {}", e.to_string())));
                            return;
                        }
                    }
                    match self.storage.update_session(stop_ev.clone()).await {
                        Ok(_) => {},
                        Err(e) => {
                            yield Err(AgentError::RunError(format!("An error occurred while starting the generation stream: {}", e.to_string())));
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
                    10
                };
                let semaphore = Arc::new(Semaphore::new(concurrency));
                for (_, (tid, name, args)) in &tool_calls {
                    match is_valid_json(args) {
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
                                    })
                                } else {
                                    AgentEventAny::SkillLoad(SkillLoadEvent {
                                        session_id: resolved_sid.clone(),
                                        turn_id: turn_id.clone(),
                                        skill_name: v["skill_name"].as_str().unwrap_or_default().to_string(),
                                    })
                                };
                                match self.storage.update_session(tc_ev.clone()).await {
                                    Ok(_) => {},
                                    Err(e) => {
                                        yield Err(AgentError::RunError(format!("An error occurred while updating the session in the storage: {}", e.to_string())));
                                        return;
                                    }
                                }
                                yield Ok(tc_ev);
                                let permit_res = semaphore.clone().acquire_owned().await;
                                let permit = match permit_res {
                                    Ok(p) => p,
                                    Err(e) => {
                                        yield Err(AgentError::RunError(format!("Error while acquiring semaphore: {}", e.to_string())));
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
                            });
                            match self.storage.update_session(ev.clone()).await {
                                Ok(_) => {},
                                Err(e) => {
                                    yield Err(AgentError::RunError(format!("An error occurred while updating the session in the storage: {}", e.to_string())));
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
                                }
                            };
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
                    content: text.clone(),
                    name: None,
                    tool_calls: Some(tool_calls.iter().map(|(_, (tid, name, args))| ToolCall {
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
