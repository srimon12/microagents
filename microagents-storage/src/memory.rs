use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    vec,
};

use microagents_events::{AgenEventAny, SessionInitEvent, types::AgentEvent};

use crate::types::AgentStorage;

#[derive(Debug)]
pub struct InMemoryAgentStorage {
    sessions: Arc<RwLock<HashMap<String, Vec<AgenEventAny>>>>,
}

impl Default for InMemoryAgentStorage {
    fn default() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl AgentStorage for InMemoryAgentStorage {
    async fn create_session(&mut self, event: SessionInitEvent) -> anyhow::Result<()> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        sessions.insert(
            event.session_id.clone(),
            vec![AgenEventAny::SessionInit(event)],
        );
        Ok(())
    }

    async fn update_session(&mut self, event: AgenEventAny) -> anyhow::Result<()> {
        let session_id = event.clone().session_id();
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let session = sessions.get_mut(&session_id);
        if let Some(s) = session {
            s.push(event);
            return Ok(());
        }
        Err(anyhow::anyhow!(
            "Could not find {session_id} among the registered sessions"
        ))
    }

    async fn get_session(&mut self, session_id: &str) -> anyhow::Result<Vec<AgenEventAny>> {
        let sessions = self
            .sessions
            .read()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let session = sessions.get(session_id);
        if let Some(s) = session {
            return Ok(s.to_owned());
        }
        Err(anyhow::anyhow!(
            "Could not find {session_id} among the registered sessions"
        ))
    }
}
