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
        if let Some(parent) = std::path::Path::new(&path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
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

#[cfg(test)]
mod tests {
    use std::fs;

    use microagents_events::{AssistantResponseEvent, SessionStopEvent, UserPromptSubmitEvent};

    use super::*;

    fn cleanup_test_file() {
        let test_fl = PathBuf::from("test.db");
        if test_fl.exists() {
            fs::remove_file(test_fl).expect("Should be able to remove file");
        }
    }

    #[tokio::test]
    async fn test_default_init() {
        let result = SqliteAgentStorage::new(None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_create_session() {
        let sql = SqliteAgentStorage::new(Some("test.db".to_string()))
            .await
            .expect("Should be able to open agent store");
        sql.create_session(SessionInitEvent {
            session_id: "1".to_string(),
            model: "gpt-5.5".into(),
            provider: "openai".into(),
            system: "you are a helpful assistant".into(),
            init_type: microagents_events::SessionInitType::Start,
        })
        .await
        .expect("Should be able to create a session");
        let rows =
            sql.connection
                .call(
                    move |conn| -> Result<Vec<(isize, String)>, tokio_rusqlite::rusqlite::Error> {
                        let mut stmt = conn.prepare(
                            "SELECT id, payload FROM events WHERE session_id = ?1 ORDER BY id ASC",
                        )?;
                        let rows = stmt
                    .query_map(["1"], |row| {
                        Ok((row.get::<_, isize>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<(isize, String)>, tokio_rusqlite::rusqlite::Error>>()?;
                        Ok(rows)
                    },
                )
                .await
                .expect("Should be able to perform sql operation");

        let events: Vec<AgentEventAny> = rows
            .into_iter()
            .map(|(_, payload)| {
                let jrpc: JsonRpcNotification = serde_json::from_str(&payload).unwrap();
                AgentEventAny::try_from(jrpc).unwrap()
            })
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].clone().to_jsonrpc().method,
            "session.init".to_string()
        );
        cleanup_test_file();
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_create_update_get_session() {
        let sql = SqliteAgentStorage::new(Some("test.db".to_string()))
            .await
            .expect("Should be able to create sqlite store");
        sql.create_session(SessionInitEvent {
            session_id: "1".to_string(),
            model: "gpt-5.5".into(),
            provider: "openai".into(),
            system: "you are a helpful assistant".into(),
            init_type: microagents_events::SessionInitType::Start,
        })
        .await
        .expect("Should be able to create a session");
        sql.update_session(AgentEventAny::UserPromptSubmit(UserPromptSubmitEvent {
            prompt: "hello".to_string(),
            session_id: "1".to_string(),
            turn_id: "t1".to_string(),
        }))
        .await
        .expect("Should be able to update memory");
        sql.update_session(AgentEventAny::AssistantResponse(AssistantResponseEvent {
            session_id: "1".to_string(),
            turn_id: "t1".to_string(),
            full_text: "hello".to_string(),
            tool_calls: None,
        }))
        .await
        .expect("Should be able to update memory");
        sql.update_session(AgentEventAny::SessionStop(SessionStopEvent {
            session_id: "1".to_string(),
            result: Some("hello".to_string()),
            error: None,
            success: true,
        }))
        .await
        .expect("Should be able to update memory");
        let events = sql
            .get_session("1")
            .await
            .expect("Should be able to get the session");
        assert_eq!(events.len(), 4);
        assert_eq!(
            events[0].clone().to_jsonrpc().method,
            "session.init".to_string()
        );
        assert_eq!(
            events[1].clone().to_jsonrpc().method,
            "user.prompt.submit".to_string()
        );
        assert_eq!(
            events[2].clone().to_jsonrpc().method,
            "assistant.response".to_string()
        );
        assert_eq!(
            events[3].clone().to_jsonrpc().method,
            "session.stop".to_string()
        );
        cleanup_test_file();
    }
}
