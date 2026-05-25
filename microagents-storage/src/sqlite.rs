use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};

use microagents_events::{
    AgentEventAny, SessionInitEvent,
    types::{AgentEvent, JsonRpcNotification},
};
use rusqlite::Connection;

use crate::types::AgentStorage;

pub static SQLITE_SESSION_STORAGE: OnceLock<PathBuf> = OnceLock::new();

pub fn sqlite_session_storage() -> &'static PathBuf {
    SQLITE_SESSION_STORAGE.get_or_init(|| {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(".microagents")
            .join("sessions.db")
    })
}

#[derive(Debug)]
pub struct SqliteAgentStorage {
    connection: Option<Arc<Mutex<Connection>>>,
}

struct SqlRowImpl {
    payload: String,
    id: isize,
}

impl Default for SqliteAgentStorage {
    fn default() -> Self {
        Self { connection: None }
    }
}

impl SqliteAgentStorage {
    fn get_connection(&mut self) -> anyhow::Result<Arc<Mutex<Connection>>> {
        if let Some(c) = self.connection.clone() {
            return Ok(c);
        }
        let conn = Connection::open(sqlite_session_storage())?;
        self.connection = Some(Arc::new(Mutex::new(conn)));
        return Ok(self.connection.clone().unwrap());
    }

    fn ensure_table_and_idx(&mut self) -> anyhow::Result<()> {
        let conn_mu = self.get_connection()?;
        let conn = conn_mu.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
        conn.execute(
            r#"CREATE TABLE events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id  TEXT    NOT NULL,
            payload     TEXT    NOT NULL,  -- JSON of the specific event
            created_at  INTEGER NOT NULL   -- unix timestamp
        );"#,
            [],
        )?;
        conn.execute(
            r#"CREATE INDEX IF NOT EXISTS idx_events_session_id ON events(session_id);"#,
            [],
        )?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AgentStorage for SqliteAgentStorage {
    async fn create_session(&mut self, event: SessionInitEvent) -> anyhow::Result<()> {
        self.ensure_table_and_idx()?;
        let conn_mu = self.get_connection()?;
        let conn = conn_mu.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let session_id = event.session_id.clone();
        let json_event = serde_json::to_string(&event.to_jsonrpc())?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis() as u32;
        conn.execute(
            r#"
        INSERT INTO events (session_id, payload, created_at) VALUES (?1, ?2, ?3)
        "#,
            (session_id, json_event, now),
        )?;
        Ok(())
    }

    async fn update_session(&mut self, event: AgentEventAny) -> anyhow::Result<()> {
        self.ensure_table_and_idx()?;
        let conn_mu = self.get_connection()?;
        let conn = conn_mu.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let session_id = event.clone().session_id();
        let json_event = serde_json::to_string(&event.to_jsonrpc())?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis() as u32;
        conn.execute(
            r#"
        INSERT INTO events (session_id, payload, created_at) VALUES (?1, ?2, ?3)
        "#,
            (session_id, json_event, now),
        )?;
        Ok(())
    }

    async fn get_session(&mut self, session_id: &str) -> anyhow::Result<Vec<AgentEventAny>> {
        self.ensure_table_and_idx()?;
        let conn_mu = self.get_connection()?;
        let conn = conn_mu.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT id, payload FROM events WHERE session_id = ?1")?;
        let rows_iter = stmt.query_map([session_id], |row| {
            Ok(SqlRowImpl {
                id: row.get(0)?,
                payload: row.get(1)?,
            })
        })?;
        let mut rows = vec![];
        for row in rows_iter {
            rows.push(row.unwrap());
        }
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        let mut events = vec![];
        for row in rows {
            let jrpc: JsonRpcNotification = serde_json::from_str(&row.payload)?;
            let ev = AgentEventAny::try_from(jrpc)?;
            events.push(ev);
        }
        Ok(events)
    }
}
