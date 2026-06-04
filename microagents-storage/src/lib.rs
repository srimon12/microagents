/// JSON Lines (JSONL) file-based session storage.
pub mod jsonl;
/// In-memory session storage.
pub mod memory;
/// SQLite-backed session storage.
pub mod sqlite;
/// Core storage trait and backend selection types.
pub mod types;
