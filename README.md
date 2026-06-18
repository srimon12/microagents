# microagents

A minimal, modular AI-agent framework written in Rust. It provides a small core library, pluggable storage backends, an event-driven architecture, and a batteries-included CLI with an embedded TUI.

> **Status:** in active development, the API might be changing in the future.

## Workspace crates

| Crate | Purpose |
|-------|---------|
| `microagents-core` | Agent definition, tool trait, skill loading, LLM generation loop |
| `microagents-events` | Event types (`AgentEventAny`, `SessionInitEvent`, …) and JSON-RPC serialization |
| `microagents-storage` | Session persistence: in-memory, JSONL, or SQLite |
| `microagents-cli` | Interactive terminal UI + built-in tools (`read`, `write`, `edit`, `shell_execute`, `search`) |

## Features

- **Multi-provider LLM support** — OpenAI (and chat completions-compatible providers), OpenRouter, Groq and Ollama out of the box (via `ultrafast-models-sdk`).
- **Tool-use loop** — the agent can call tools, wait for results, and continue the conversation. Supports parallel tool calls.
- **Skills** — drop-in markdown skill packs with front-matter (`name`, `description`, `allowed-tools`, …) loaded from `./.agents/skills` or `~/.agents/skills`.
- **AGENTS.md** — Load an AGENTS.md as part of your agent's custom instructions
- **Session storage** — resume conversations by persisting events to JSONL files, SQLite, or keep them in memory (not recommended).
- **Built-in tools for `microagents-cli`**
  - `read` — read plain text or extract text from PDFs / Office docs / images (via `liteparse`).
  - `write` — create or overwrite files (with auto-created parent directories).
  - `edit` — exact-string replacement in a file.
  - `shell_execute` — run shell commands with a dangerous-command regex guard.
  - `search` — semantic (code and text embeddings) + sparse (BM25) hybrid search over the workspace, powered by `qdrant-edge`, `fastembed-rs` and `astchunk`, using TurboQuant quantization.
- **Embedded TUI for `microagents-cli`** — `ratatui`-based chat interface with streaming deltas, tool-call visualization, scrolling history, and session resume.

## Compatibility

Rust 1.91.1+ is necessary.

## Quick start
 
> _Installation from crates.io coming soon!_

Clone the repository and install from source:

```bash
cargo +nightly install microagents-cli
```

This will install the `microag` binary.

Run the TUI with default settings:

```bash
microag
```

Run the TUI customizing model/provider/storage backend:

```bash
microag --provider ollama --model gemma4:latest --storage sqlite
```

Run the TUI resuming a previous session:

```bash
microag --session-id <session-id>
```

Run in headless mode, printing JSON-RPC-serialized events to the console, using the `-p/--prompt` flag:

```bash
microag -p 'Where is AgentStorage defined and which storage backends implement it?'
```

Run in verbose mode to see codebase indexing progression:

```bash
microag --verbose -p 'Where is AgentStorage defined and which storage backends implement it?'
```

Infer provider from the environment:

```bash
export OPENROUTER_API_KEY="..."
# uses OpenRouter automatically
microag --model "anthropic/claude-opus-4.6"
```

## Architecture overview

```
┌─────────────────┐
│  microagents-cli │  ← TUI + built-in tools + vector search indexing
│  (binary: microag)│
└────────┬────────┘
         │
┌────────▼────────┐
│ microagents-core │  ← Agent trait, MicroAgent builder, tool loop, skill loader
└────────┬────────┘
         │
┌────────▼────────┐
│microagents-events│  ← Structured events → JSON-RPC notifications
└────────┬────────┘
         │
┌────────▼────────┐
│microagents-storage│ ← In-memory / JSONL / SQLite session backends
└─────────────────┘
```

## Programmatic usage

Add the libraries with:

```bash
cargo add microagents-events microagents-storage microagents-core
```

### Creating an agent

```rust
use microagents_core::{
    agent::{MicroAgentBuilder, SupportedProvider},
    types::{Agent, ToolExecutionContext},
};
use microagents_storage::types::AgentStorageChoice;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
        .provider("openrouter".to_string())?
        .model("anthropic/claude-opus-4.7".to_string())
        .storage(AgentStorageChoice::Sqlite)
        .await?
        .find_skills()?
        .load_agents_md()?
        .build()?;

    let mut stream = agent.run("Hello!".to_string(), None).await?;

    while let Some(event) = stream.next().await {
        match event {
            Ok(ev) => println!("{:?}", ev),
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    Ok(())
}
```

### Custom tools

Implement `ToolFunction<Ctx>` from `microagents_core::types` and register it on the `MicroAgentBuilder`:

```rust
use microagents_core::types::{ToolFunction, ToolExecutionContext, AgentError};
use microagents_events::types::ToolResult;
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug)]
struct MyTool;

#[async_trait::async_trait]
impl ToolFunction<()> for MyTool {
    fn name(&self) -> &'static str { "my_tool" }
    fn description(&self) -> &'static str { "Does something useful" }
    fn input_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }
    async fn execute(&self, input: Value, _ctx: &Arc<ToolExecutionContext<()>>) -> Result<ToolResult, AgentError> {
        Ok(ToolResult::Ok("done".into()))
    }
}

// Then add it when building the agent:
// let agent = MicroAgentBuilder::new(...)
//     .add_tool(Arc::new(MyTool))?
//     .build()?;
```


## Environment variables

| Variable | Required for | Description |
|----------|--------------|-------------|
| `OPENROUTER_API_KEY` | OpenRouter | API key for OpenRouter (default provider) |
| `OPENAI_API_KEY` | OpenAI | API key for OpenAI |
| `OPENAI_API_KEY` and `OPENAI_BASE_URL` | OpenAI-Compatible providers | API key and base URL for OpenAI-compatible providers |
| `GROQ_API_KEY` | Groq | API key for Groq |
| `OLLAMA_BASE_URL` | Ollama | Base URL for Ollama (defaults to `http://localhost:11434/v1`) |

## License

MIT
