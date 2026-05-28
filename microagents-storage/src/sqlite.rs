use crate::types::AgentStorage;
use microagents_events::{
    AgentEventAny, SessionInitEvent,
    types::{AgentEvent, JsonRpcNotification},
};
use std::{path::PathBuf, sync::OnceLock, time::SystemTime};
use tokio_rusqlite::Connection;
use tokio_rusqlite::rusqlite;

pub static SQLITE_SESSION_STORAGE: OnceLock<PathBuf> = OnceLock::new();

pub fn sqlite_session_storage() -> &'static PathBuf {
    SQLITE_SESSION_STORAGE.get_or_init(|| {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(".microagents")
            .join("sessions.db")
    })
}

#[derive(Debug, Clone)]
pub struct SqliteAgentStorage {
    connection: Connection,
}

impl SqliteAgentStorage {
    pub async fn new(db_path: Option<String>) -> anyhow::Result<Self> {
        let path = db_path.unwrap_or(sqlite_session_storage().to_string_lossy().to_string());
        let connection = Connection::open(path).await?;
        let storage = Self { connection };
        storage.ensure_table_and_idx().await?;
        Ok(storage)
    }

    async fn ensure_table_and_idx(&self) -> anyhow::Result<()> {
        self.connection
            .call(|conn| -> Result<(), tokio_rusqlite::rusqlite::Error> {
                conn.execute_batch(
                    r#"
                CREATE TABLE IF NOT EXISTS events (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id  TEXT    NOT NULL,
                    payload     TEXT    NOT NULL,
                    created_at  INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_events_session_id ON events(session_id);
            "#,
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AgentStorage for SqliteAgentStorage {
    async fn create_session(&self, event: SessionInitEvent) -> anyhow::Result<()> {
        let session_id = event.session_id.clone();
        let json_event = serde_json::to_string(&event.to_jsonrpc())?;
        let now = now_millis()?;

        self.connection
            .call(move |conn| -> Result<(), tokio_rusqlite::rusqlite::Error> {
                conn.execute(
                    "INSERT INTO events (session_id, payload, created_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![session_id, json_event, now],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    async fn update_session(&self, event: AgentEventAny) -> anyhow::Result<()> {
        let session_id = event.clone().session_id();
        let json_event = serde_json::to_string(&event.to_jsonrpc())?;
        let now = now_millis()?;

        self.connection
            .call(move |conn| -> Result<(), tokio_rusqlite::rusqlite::Error> {
                conn.execute(
                    "INSERT INTO events (session_id, payload, created_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![session_id, json_event, now],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    async fn get_session(&self, session_id: &str) -> anyhow::Result<Vec<AgentEventAny>> {
        let session_id = session_id.to_string();

        let rows =
            self.connection
                .call(
                    move |conn| -> Result<Vec<(isize, String)>, tokio_rusqlite::rusqlite::Error> {
                        let mut stmt = conn.prepare(
                            "SELECT id, payload FROM events WHERE session_id = ?1 ORDER BY id ASC",
                        )?;
                        let rows = stmt
                    .query_map([&session_id], |row| {
                        Ok((row.get::<_, isize>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<(isize, String)>, tokio_rusqlite::rusqlite::Error>>()?;
                        Ok(rows)
                    },
                )
                .await?;

        rows.into_iter()
            .map(|(_, payload)| {
                let jrpc: JsonRpcNotification = serde_json::from_str(&payload)?;
                AgentEventAny::try_from(jrpc).map_err(|e| anyhow::anyhow!(e.to_string()))
            })
            .collect()
    }
}

fn now_millis() -> anyhow::Result<i64> {
    Ok(SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_millis() as i64)
}
