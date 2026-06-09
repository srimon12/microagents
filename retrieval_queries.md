# Retrieval Query Battery — MicroAgents Rust Codebase

Below are 30 retrieval queries focused on the Rust code, each with up to 5 relevant code locations.

---

## Query 1
**Query:** How is the `MicroAgentBuilder` struct defined and what fields does it contain?

**Relevant code:**
1. `microagents-core/src/agent.rs` lines 142-155 — `MicroAgentBuilder` struct definition
2. `microagents-core/src/agent.rs` lines 157-175 — `MicroAgentBuilder::new()` constructor
3. `microagents-core/src/agent.rs` lines 177-186 — `add_skill()` method
4. `microagents-core/src/agent.rs` lines 188-197 — `find_skills()` method
5. `microagents-core/src/agent.rs` lines 199-203 — `provider()` method

---

## Query 2
**Query:** What is the `Agent` trait and what methods must be implemented?

**Relevant code:**
1. `microagents-core/src/types.rs` lines 58-70 — `Agent` trait definition with `generate` and `run`
2. `microagents-core/src/agent.rs` lines 465-725 — `Agent` implementation for `MicroAgent`
3. `microagents-core/src/agent.rs` lines 727-753 — `DummyAgent` mock implementing `Agent`
4. `microagents-core/tests/integration_test.rs` lines 40-67 — `WeatherTool` implementing `ToolFunction`
5. `microagents-core/src/types.rs` lines 74-90 — `ToolExecutionContext` definition

---

## Query 3
**Query:** How does the agent handle parallel tool calls and what is the concurrency limit?

**Relevant code:**
1. `microagents-core/src/agent.rs` lines 56-58 — `MAX_CONCURRENT_TOOL_CALLS` constant
2. `microagents-core/src/agent.rs` lines 536-539 — semaphore creation with concurrency limit
3. `microagents-core/src/agent.rs` lines 541-585 — `JoinSet` spawning tool calls with semaphore
4. `microagents-core/src/agent.rs` lines 143-155 — `parallel_tool_calls` field in builder
5. `microagents-core/src/agent.rs` lines 215-219 — `parallel_tool_calls()` builder method

---

## Query 4
**Query:** How are skills loaded and parsed from disk?

**Relevant code:**
1. `microagents-core/src/skills.rs` lines 53-72 — `parse_skill()` function
2. `microagents-core/src/skills.rs` lines 74-88 — `ensure_skill()` function
3. `microagents-core/src/skills.rs` lines 90-116 — `find_skills()` function
4. `microagents-core/src/skills.rs` lines 16-22 — `global_skills_path()` using `OnceLock`
5. `microagents-core/src/agent.rs` lines 177-186 — `add_skill()` on builder

---

## Query 5
**Query:** What is the `SupportedProvider` enum and what are its default models?

**Relevant code:**
1. `microagents-core/src/agent.rs` lines 60-70 — `SupportedProvider` enum definition
2. `microagents-core/src/agent.rs` lines 72-86 — `FromStr` implementation
3. `microagents-core/src/agent.rs` lines 88-100 — `Display` implementation
4. `microagents-core/src/agent.rs` lines 102-121 — `default_model()` method
5. `microagents-core/src/agent.rs` lines 123-138 — `MicroAgentBuilderError` enum

---

## Query 6
**Query:** How does the TUI render agent events and handle user input?

**Relevant code:**
1. `microagents-cli/src/tui/mod.rs` lines 85-109 — `App` struct and state management
2. `microagents-cli/src/tui/mod.rs` lines 111-140 — `apply_agent_event()` method
3. `microagents-cli/src/tui/mod.rs` lines 305-390 — `handle_key()` function
4. `microagents-cli/src/tui/mod.rs` lines 470-540 — `draw_transcript()` rendering
5. `microagents-cli/src/tui/mod.rs` lines 592-650 — `draw_input()` input box rendering

---

## Query 7
**Query:** How is the `AgentStorage` trait defined and what implementations exist?

**Relevant code:**
1. `microagents-storage/src/types.rs` lines 10-20 — `AgentStorage` trait definition
2. `microagents-storage/src/types.rs` lines 22-31 — `AgentStorageChoice` enum
3. `microagents-storage/src/memory.rs` lines 15-55 — `InMemoryAgentStorage` implementation
4. `microagents-storage/src/jsonl.rs` lines 24-95 — `JsonlAgentStorage` implementation
5. `microagents-storage/src/sqlite.rs` lines 24-145 — `SqliteAgentStorage` implementation

---

## Query 8
**Query:** How does the SQLite storage backend persist and retrieve session events?

**Relevant code:**
1. `microagents-storage/src/sqlite.rs` lines 24-48 — `SqliteAgentStorage::new()` constructor
2. `microagents-storage/src/sqlite.rs` lines 50-78 — `ensure_table_and_idx()` schema setup
3. `microagents-storage/src/sqlite.rs` lines 80-97 — `create_session()` implementation
4. `microagents-storage/src/sqlite.rs` lines 99-116 — `update_session()` implementation
5. `microagents-storage/src/sqlite.rs` lines 118-145 — `get_session()` implementation

---

## Query 9
**Query:** How are agent events converted to and from JSON-RPC notifications?

**Relevant code:**
1. `microagents-events/src/types.rs` lines 78-95 — `JsonRpcNotification` struct and builder
2. `microagents-events/src/types.rs` lines 97-103 — `AgentEvent` trait
3. `microagents-events/src/lib.rs` lines 200-230 — `SessionInitEvent::to_jsonrpc()`
4. `microagents-events/src/lib.rs` lines 380-500 — `AgentEventAny::try_from(JsonRpcNotification)`
5. `microagents-events/src/lib.rs` lines 340-360 — `AgentEventAny::to_jsonrpc()` dispatch

---

## Query 10
**Query:** How does the `call_tool` function validate and execute tool calls?

**Relevant code:**
1. `microagents-core/src/common.rs` lines 95-106 — `call_tool()` function
2. `microagents-core/src/common.rs` lines 76-85 — `JsonResult` enum for partial JSON
3. `microagents-core/src/common.rs` lines 87-93 — `parse_json_fragment()` function
4. `microagents-core/src/types.rs` lines 93-117 — `ToolFunction` trait definition
5. `microagents-core/src/agent.rs` lines 541-585 — tool call execution loop in `run()`

---

## Query 11
**Query:** How does the `EditTool` perform atomic file edits?

**Relevant code:**
1. `microagents-cli/src/tools.rs` lines 295-390 — `EditTool` implementation
2. `microagents-cli/src/tools.rs` lines 340-345 — uniqueness check for `old_str`
3. `microagents-cli/src/tools.rs` lines 349-352 — atomic write with `NamedTempFile`
4. `microagents-cli/src/tools.rs` lines 354-370 — permission preservation
5. `microagents-cli/src/tools.rs` lines 372-374 — `tmp.persist()` atomic replacement

---

## Query 12
**Query:** How does the `ShellExecuteTool` detect and block dangerous commands?

**Relevant code:**
1. `microagents-cli/src/tools.rs` lines 25-85 — `dangerous_regex()` regex compilation
2. `microagents-cli/src/tools.rs` lines 87-91 — `is_dangerous()` function
3. `microagents-cli/src/tools.rs` lines 392-480 — `ShellExecuteTool::execute()`
4. `microagents-cli/src/tools.rs` lines 410-414 — dangerous command check
5. `microagents-cli/src/tools.rs` lines 416-424 — platform-specific command spawning

---

## Query 13
**Query:** How does the vector search system chunk and embed code files?

**Relevant code:**
1. `microagents-cli/src/processing.rs` lines 45-65 — `infer_language_from_extension()`
2. `microagents-cli/src/processing.rs` lines 67-90 — `chunk_code()` using AST chunker
3. `microagents-cli/src/processing.rs` lines 92-102 — `chunk_text()` for non-code
4. `microagents-cli/src/processing.rs` lines 112-124 — `embed()` dense + sparse embeddings
5. `microagents-cli/src/processing.rs` lines 126-132 — `embed_query()` for search queries

---

## Query 14
**Query:** How is the Qdrant Edge vector store configured and queried?

**Relevant code:**
1. `microagents-cli/src/init_env.rs` lines 35-68 — `edge_config()` with quantization
2. `microagents-cli/src/init_env.rs` lines 198-210 — `load_qdrant_edge()` loader
3. `microagents-cli/src/search.rs` lines 23-85 — `search()` with RRF fusion
4. `microagents-cli/src/search.rs` lines 35-75 — sparse + dense prefetch queries
5. `microagents-cli/src/init_env.rs` lines 28-33 — vector names and size constants

---

## Query 15
**Query:** How does the `ReadTool` handle both text files and unstructured documents?

**Relevant code:**
1. `microagents-cli/src/tools.rs` lines 135-230 — `ReadTool::execute()`
2. `microagents-cli/src/tools.rs` lines 155-165 — `SUPPORTED_LIT_EXTENSIONS` check
3. `microagents-cli/src/tools.rs` lines 167-186 — LiteParse document parsing
4. `microagents-cli/src/tools.rs` lines 188-213 — text file reading with offset/max_length
5. `microagents-cli/src/init_env.rs` lines 20-24 — `SUPPORTED_LIT_EXTENSIONS` array

---

## Query 16
**Query:** How does the environment initialization detect file changes and re-ingest?

**Relevant code:**
1. `microagents-cli/src/init_env.rs` lines 110-140 — `Document` struct and `Diff` logic
2. `microagents-cli/src/init_env.rs` lines 142-175 — `diff_files()` comparison
3. `microagents-cli/src/init_env.rs` lines 177-196 — `collect_files()` with ignore patterns
4. `microagents-cli/src/init_env.rs` lines 290-350 — `ingest_files()` concurrent ingestion
5. `microagents-cli/src/init_env.rs` lines 352-370 — `delete_files()` from vector store

---

## Query 17
**Query:** How does the `AgentError` enum represent different failure modes?

**Relevant code:**
1. `microagents-core/src/types.rs` lines 10-36 — `AgentError` enum definition
2. `microagents-core/src/types.rs` lines 16-18 — `ToolCallError` variant
3. `microagents-core/src/types.rs` lines 20-22 — `RunError` variant
4. `microagents-core/src/types.rs` lines 24-26 — `ClientInitFailed` variant
5. `microagents-core/src/types.rs` lines 32-34 — `TokenizerLoadingError` variant

---

## Query 18
**Query:** How does the `SkillsTool` load skill instructions at runtime?

**Relevant code:**
1. `microagents-core/src/agent.rs` lines 352-365 — `SkillsTool` struct definition
2. `microagents-core/src/agent.rs` lines 367-395 — `SkillsTool` `ToolFunction` implementation
3. `microagents-core/src/agent.rs` lines 385-391 — reading `SKILL.md` from disk
4. `microagents-core/src/skills.rs` lines 74-88 — `ensure_skill()` path resolution
5. `microagents-core/src/agent.rs` lines 47-48 — `SKILLS_TOOL_NAME` constant

---

## Query 19
**Query:** How does the TUI theme define colors for different message types?

**Relevant code:**
1. `microagents-cli/src/tui/mod.rs` lines 55-68 — `theme` module with color constants
2. `microagents-cli/src/tui/mod.rs` lines 56-57 — `ACCENT` and `ACCENT_SOFT` blues
3. `microagents-cli/src/tui/mod.rs` lines 58-60 — `USER`, `ASSISTANT`, `THINKING` colors
4. `microagents-cli/src/tui/mod.rs` lines 61-63 — `TOOL`, `TOOL_OK`, `TOOL_ERR` colors
5. `microagents-cli/src/tui/mod.rs` lines 64-65 — `SKILL` and `ERROR` colors

---

## Query 20
**Query:** How does the `MicroAgent::generate()` method stream LLM responses?

**Relevant code:**
1. `microagents-core/src/agent.rs` lines 397-426 — `generate()` implementation
2. `microagents-core/src/agent.rs` lines 404-418 — `ChatRequest` construction with tools
3. `microagents-core/src/agent.rs` lines 420-425 — stream mapping with error conversion
4. `microagents-core/src/agent.rs` lines 268-275 — `init_client()` lazy initialization
5. `microagents-core/src/types.rs` lines 38-42 — `GenerationStream` type alias

---

## Query 21
**Query:** How does the `WriteTool` create files and parent directories?

**Relevant code:**
1. `microagents-cli/src/tools.rs` lines 232-293 — `WriteTool` implementation
2. `microagents-cli/src/tools.rs` lines 270-275 — `fs::create_dir_all()` for parents
3. `microagents-cli/src/tools.rs` lines 277-282 — `fs::write()` atomic write
4. `microagents-cli/src/tools.rs` lines 255-260 — workspace boundary check via `is_within_root()`
5. `microagents-cli/src/tools.rs` lines 93-113 — `is_within_root()` implementation

---

## Query 22
**Query:** How does the JSONL storage backend read and write session files?

**Relevant code:**
1. `microagents-storage/src/jsonl.rs` lines 24-35 — `JsonlAgentStorage` struct and default
2. `microagents-storage/src/jsonl.rs` lines 49-61 — `create_session()` append write
3. `microagents-storage/src/jsonl.rs` lines 63-76 — `update_session()` append write
4. `microagents-storage/src/jsonl.rs` lines 78-95 — `get_session()` line-by-line read
5. `microagents-storage/src/jsonl.rs` lines 88-94 — corrupted line handling with `eprintln`

---

## Query 23
**Query:** How does the `convert_event_to_message` function map events to LLM messages?

**Relevant code:**
1. `microagents-core/src/common.rs` lines 46-93 — `convert_event_to_message()` function
2. `microagents-core/src/common.rs` lines 48-56 — `UserPromptSubmit` → `Role::User`
3. `microagents-core/src/common.rs` lines 57-78 — `AssistantResponse` → `Role::Assistant` with tool calls
4. `microagents-core/src/common.rs` lines 79-93 — `ToolResult` → `Role::Tool`
5. `microagents-core/src/common.rs` lines 90-92 — `_ => None` for non-message events

---

## Query 24
**Query:** How does the TUI handle session history loading on resume?

**Relevant code:**
1. `microagents-cli/src/tui/mod.rs` lines 185-215 — `run_with_session()` entry point
2. `microagents-cli/src/tui/mod.rs` lines 187-198 — `load_history()` call for resume
3. `microagents-cli/src/tui/mod.rs` lines 222-226 — history events applied to transcript
4. `microagents-cli/src/tui/mod.rs` lines 111-140 — `apply_agent_event()` with `is_replay` flag
5. `microagents-cli/src/main.rs` lines 120-130 — `build_storage()` for history loading

---

## Query 25
**Query:** How does the `SearchTool` perform semantic search across the workspace?

**Relevant code:**
1. `microagents-cli/src/tools.rs` lines 117-133 — `SearchTool` struct and metadata
2. `microagents-cli/src/tools.rs` lines 160-195 — `SearchTool::execute()` implementation
3. `microagents-cli/src/tools.rs` lines 175-180 — `embed_query()` call
4. `microagents-cli/src/tools.rs` lines 182-194 — `vector_search()` call and result formatting
5. `microagents-cli/src/search.rs` lines 23-85 — `search()` function with fusion

---

## Query 26
**Query:** How does the CLI `main.rs` parse arguments and build the agent?

**Relevant code:**
1. `microagents-cli/src/main.rs` lines 15-45 — `Args` struct with clap derives
2. `microagents-cli/src/main.rs` lines 47-58 — `storage_choice()` parser
3. `microagents-cli/src/main.rs` lines 68-93 — `build_agent()` function
4. `microagents-cli/src/main.rs` lines 95-115 — headless mode execution
5. `microagents-cli/src/main.rs` lines 117-135 — TUI mode with closures

---

## Query 27
**Query:** How does the `InMemoryAgentStorage` maintain session state in memory?

**Relevant code:**
1. `microagents-storage/src/memory.rs` lines 10-14 — `InMemoryAgentStorage` struct with `RwLock`
2. `microagents-storage/src/memory.rs` lines 18-22 — `Default` implementation
3. `microagents-storage/src/memory.rs` lines 24-33 — `create_session()` insert
4. `microagents-storage/src/memory.rs` lines 35-45 — `update_session()` append
5. `microagents-storage/src/memory.rs` lines 47-58 — `get_session()` read and sort

---

## Query 28
**Query:** How does the `Usage` struct track token and latency metrics?

**Relevant code:**
1. `microagents-events/src/lib.rs` lines 85-93 — `Usage` struct definition
2. `microagents-events/src/lib.rs` lines 86-87 — `latency` and `input_chars` fields
3. `microagents-events/src/lib.rs` lines 88-89 — `estimated_input_tokens` field
4. `microagents-events/src/lib.rs` lines 90-91 — `output_chars` and `estimated_output_tokens`
5. `microagents-core/src/agent.rs` lines 650-665 — `Usage` construction in stop event

---

## Query 29
**Query:** How does the `MicroAgentBuilder::resolve_system()` assemble the system prompt?

**Relevant code:**
1. `microagents-core/src/agent.rs` lines 233-264 — `resolve_system()` method
2. `microagents-core/src/agent.rs` lines 235-243 — base prompt + model/provider info
3. `microagents-core/src/agent.rs` lines 244-256 — tools XML serialization
4. `microagents-core/src/agent.rs` lines 257-264 — skills XML serialization
5. `microagents-core/src/agent.rs` lines 31-44 — `BASE_SYSTEM_PROMPT` constant

---

## Query 30
**Query:** How does the `AgentEventAny` enum wrap all possible event types?

**Relevant code:**
1. `microagents-events/src/lib.rs` lines 318-331 — `AgentEventAny` enum definition
2. `microagents-events/src/lib.rs` lines 333-345 — `timestamp()` method dispatch
3. `microagents-events/src/lib.rs` lines 347-360 — `to_jsonrpc()` dispatch
4. `microagents-events/src/lib.rs` lines 362-375 — `session_id()` dispatch
5. `microagents-events/src/lib.rs` lines 377-500 — `TryFrom<JsonRpcNotification>` implementation

---

i am done
