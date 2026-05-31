use fs2::FileExt;
use microagents_events::types::{AgentEvent, JsonRpcNotification};
use microagents_events::{AgentEventAny, SessionInitEvent};
use std::io::Read;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::OnceLock,
};

use crate::types::AgentStorage;

pub static JSONL_SESSION_STORAGE: OnceLock<PathBuf> = OnceLock::new();

pub fn jsonl_session_storage() -> &'static PathBuf {
    JSONL_SESSION_STORAGE.get_or_init(|| {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(".microagents")
            .join("sessions")
    })
}

#[derive(Debug)]
pub struct JsonlAgentStorage;

impl JsonlAgentStorage {
    fn ensure_sessions_dir(&self) -> anyhow::Result<()> {
        if jsonl_session_storage().exists() {
            return Ok(());
        }
        fs::create_dir_all(jsonl_session_storage())?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AgentStorage for JsonlAgentStorage {
    async fn create_session(&self, event: SessionInitEvent) -> anyhow::Result<()> {
        self.ensure_sessions_dir()?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(jsonl_session_storage().join(format!("{}.jsonl", event.session_id)))?;
        let event_json = serde_json::to_string(&event.to_jsonrpc())?;
        file.lock_exclusive()?;
        writeln!(file, "{}", event_json)?;
        file.unlock()?;
        Ok(())
    }

    async fn update_session(&self, event: AgentEventAny) -> anyhow::Result<()> {
        self.ensure_sessions_dir()?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(jsonl_session_storage().join(format!("{}.jsonl", event.clone().session_id())))?;
        file.lock_exclusive()?;
        writeln!(file, "{}", serde_json::to_string(&event.to_jsonrpc())?)?;
        file.unlock()?;
        Ok(())
    }

    async fn get_session(&self, session_id: &str) -> anyhow::Result<Vec<AgentEventAny>> {
        self.ensure_sessions_dir()?;
        let mut file = OpenOptions::new()
            .read(true)
            .open(jsonl_session_storage().join(format!("{session_id}.jsonl")))?;
        file.lock_shared()?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        file.unlock()?;
        let mut events = vec![];
        for line in buf.lines() {
            let jsrpc: JsonRpcNotification = serde_json::from_str(line)?;
            let event = AgentEventAny::try_from(jsrpc)?;
            events.push(event);
        }

        Ok(events)
    }
}
