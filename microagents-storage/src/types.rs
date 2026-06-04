use std::fmt::Debug;

use microagents_events::{AgentEventAny, SessionInitEvent};

#[async_trait::async_trait]
pub trait AgentStorage: Send + Debug + Sync {
    async fn create_session(&self, event: SessionInitEvent) -> anyhow::Result<()>;
    async fn update_session(&self, event: AgentEventAny) -> anyhow::Result<()>;
    async fn get_session(&self, session_id: &str) -> anyhow::Result<Vec<AgentEventAny>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum AgentStorageChoice {
    Memory,
    Jsonl,
    Sqlite,
}
