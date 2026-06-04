use std::fmt::Debug;

use microagents_events::{AgentEventAny, SessionInitEvent};

/// Trait for backends that persist and retrieve agent session events.
#[async_trait::async_trait]
pub trait AgentStorage: Send + Debug + Sync {
    /// Persist the initial event that starts a new session.
    async fn create_session(&self, event: SessionInitEvent) -> anyhow::Result<()>;
    /// Append an event to an existing session.
    async fn update_session(&self, event: AgentEventAny) -> anyhow::Result<()>;
    /// Retrieve all events for a given session, ordered by time.
    async fn get_session(&self, session_id: &str) -> anyhow::Result<Vec<AgentEventAny>>;
}

/// Available storage backend implementations.
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum AgentStorageChoice {
    /// In-memory storage (ephemeral).
    Memory,
    /// JSON Lines file storage.
    Jsonl,
    /// SQLite database storage.
    Sqlite,
}
