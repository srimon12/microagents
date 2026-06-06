# microagents-cli

A batteries-included, terminal-based AI agent built on top of the **microagents** framework.

## What it is

`microagents-cli` (binary: `microag`) is a Rust CLI that gives you an interactive coding agent inside your terminal. It combines a local vector search index with an LLM-powered agent that can read, write, edit, search, and execute shell commands in your workspace.

## Key features

- **Interactive TUI** — Chat with the agent in a minimal, keyboard-driven terminal UI built with [Ratatui](https://github.com/ratatui/ratatui). Supports multi-line input, scrollback, and session resume.
- **Headless mode** — Run a single prompt and get JSON-RPC output for scripting or piping.
- **Local semantic search** — Automatically indexes files in your workspace using dense + sparse embeddings (Model2Vec + BM25) and stores them in an on-disk [Qdrant Edge](https://qdrant.tech/documentation/edge/) vector database.
- **Smart file ingestion** — Parses code with AST-aware chunking (via `astchunk`) and unstructured documents (PDFs, Office files, images) via `liteparse`.
- **Built-in tools** — The agent can:
  - `read` — Read any file (including PDFs, Word docs, images via OCR)
  - `write` — Create or overwrite files
  - `edit` — Make precise text replacements
  - `search` — Semantic search across the indexed workspace
  - `shell_execute` — Run shell commands with a safety regex filter
- **Session persistence** — Save conversation history to JSONL or SQLite and resume later.
- **Skills system** — Auto-discover or explicitly load agent skills from the microagents framework.

## Usage

### Interactive TUI

```bash
microag
```

### Headless / single prompt

```bash
microag --prompt "Refactor the auth module to use async/await"
```

### Resume a session

```bash
microag --session-id <uuid>
```

### Options

| Flag | Description |
|------|-------------|
| `--model` | LLM model to use |
| `--provider` | Provider (defaults to `openrouter`) |
| `--storage` | `jsonl` or `sqlite` (default: `jsonl`) |
| `--skill` | Load specific skills (repeatable) |
| `--prompt` | Run in headless mode |
| `--verbose` | Print initialization info |

## Workspace indexing

On startup, the CLI scans your current directory (respecting `.microagentsignore`), computes a diff against the last run, and incrementally updates the local vector index stored in `.microagents/`. This lets the agent search and reason over your entire codebase.

## Architecture

- `src/main.rs` — CLI argument parsing, agent builder, TUI/headless dispatch
- `src/tui/mod.rs` — Full Ratatui chat interface with streaming output
- `src/init_env.rs` — File walking, diffing, ingestion, and vector store setup
- `src/processing.rs` — Chunking (AST + text) and embedding (dense + sparse)
- `src/search.rs` — Hybrid dense/sparse vector search with RRF fusion
- `src/tools.rs` — Tool implementations: read, write, edit, search, shell_execute

## License

MIT
