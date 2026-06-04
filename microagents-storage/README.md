# microagents-storage

Session storage layer for the [microagents](https://github.com/AstraBert/microagents) framework.

## Overview

This crate provides pluggable backends for persisting and retrieving agent session events. All backends implement the [`AgentStorage`](src/types.rs) trait, which defines three core operations:

- `create_session` – persist the initial event that starts a new session
- `update_session` – append an event to an existing session
- `get_session` – retrieve all events for a given session, ordered by time

## Backends

| Backend | Module | Persistence |
|---------|--------|-------------|
| **In-Memory** | [`memory`](src/memory.rs) | Ephemeral (data lost on process exit) |
| **JSON Lines** | [`jsonl`](src/jsonl.rs) | One `.jsonl` file per session in `~/.microagents/sessions` |
| **SQLite** | [`sqlite`](src/sqlite.rs) | Single SQLite database at `~/.microagents/sessions.db` |

Backend selection is controlled by [`AgentStorageChoice`](src/types.rs).

## Usage

```rust
use microagents_storage::{sqlite::SqliteAgentStorage, types::AgentStorage};

let storage = SqliteAgentStorage::new(None).await?;
storage.create_session(init_event).await?;
storage.update_session(event).await?;
let events = storage.get_session("session-id").await?;
```

## License

MIT
