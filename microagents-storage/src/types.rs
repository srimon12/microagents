use std::fmt::Debug;

use microagents_events::{AgenEventAny, SessionInitEvent};

#[async_trait::async_trait]
pub trait AgentStorage: Send + Debug {
    async fn create_session(&mut self, event: SessionInitEvent) -> anyhow::Result<()>;
    async fn update_session(&mut self, event: AgenEventAny) -> anyhow::Result<()>;
    async fn get_session(&mut self, session_id: &str) -> anyhow::Result<Vec<AgenEventAny>>;
}

pub enum AgentStorageChoice {
    Memory,
    Jsonl,
    Sqlite,
}
