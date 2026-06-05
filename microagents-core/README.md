# microagents-core

Core agent runtime for the **MicroAgents** ecosystem. It provides the building blocks for creating LLM-powered agents that can use tools, load domain-specific skills, and persist conversation sessions across multiple storage backends.

## What it does

- **Agent construction** — configure an agent via a fluent builder, choosing the LLM provider, model, tools, skills, and storage backend.
- **Streaming generation** — talk to OpenAI, Groq, OpenRouter, or a local Ollama instance and receive token-level streaming responses.
- **Tool calling** — register Rust implementations of [`ToolFunction`](src/types.rs); the runtime validates JSON arguments against a schema, executes tools (optionally in parallel), and feeds results back to the model.
- **Skills** — load markdown-based skill definitions from `.agents/skills` or `~/.agents/skills`. Skills are injected into the system prompt so the model can reason about specialised domains.
- **Session persistence** — resume conversations from JSONL, SQLite, or in-memory storage.

## Quick example

```rust
use microagents_core::agent::MicroAgentBuilder;
use microagents_core::types::ToolExecutionContext;

let agent = MicroAgentBuilder::new(ToolExecutionContext::new(()))
    .provider("openai".into())?
    .model("gpt-5.5".into())
    .find_skills()?
    .build()?;

let mut stream = agent.run("Hello!".into(), None).await?;
while let Some(event) = stream.next().await {
    println!("{:?}", event?);
}
```

## Module overview

| Module | Purpose |
|--------|---------|
| [`agent`](src/agent.rs) | `MicroAgent` / `MicroAgentBuilder`, provider configuration, and the main `Agent` trait implementation. |
| [`types`](src/types.rs) | Core types: `Agent` trait, `ToolFunction`, `ToolExecutionContext`, error types, and stream aliases. |
| [`common`](src/common.rs) | Shared helpers: event-to-message conversion, JSON fragment parsing, and the tool call dispatcher. |
| [`skills`](src/skills.rs) | Skill discovery and front-matter parsing from markdown files. |

## Supported providers

- **OpenAI** — requires `OPENAI_API_KEY`
- **Groq** — requires `GROQ_API_KEY`
- **OpenRouter** — requires `OPENROUTER_API_KEY`
- **Ollama** — reads `OLLAMA_BASE_URL` (defaults to `http://localhost:11434/v1`)
