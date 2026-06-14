# Benchmark: MicroAgents CLI Question Answering

This repository benchmarks MicroAgents CLI's ability to answer questions from corporate sustainability / ESG PDFs using the [pdfQA-Benchmark](https://huggingface.co/datasets/pdfqa/pdfQA-Benchmark) dataset.

## Overview

The benchmark downloads publicly available sustainability reports (PDFs) and their corresponding annotated question-answer pairs, then runs MicroAgent with a python script through JSONRPC over stdio. Responses are saved as raw text and translated to structured JSON answers, and the full interaction traces are recorded as well.

## Repository Structure

```
.
├── data/                          # Downloaded PDFs (gitignored)
├── questions/                     # Annotated Q&A JSON files (gitignored)
├── answers/
│   ├── raw/                       # Answers from raw PDF reading (no skill)
│   └── microag/                   # Answers from the MicroAgents harness
├── traces/
│   └── microag/                   # MicroAgents JSONL event traces
├── evaluations/                   # LLM-as-judge evaluation outputs, per mode
├── scripts/
│   └── run_bench_microag.sh
├── src/benchmark_claude_pdfs/
│   ├── download_data.py           # Download PDFs from Hugging Face
│   ├── download_qa.py             # Download Q&A annotations from Hugging Face
│   ├── run_microag.py             # Benchmark runner driving the `microag` CLI
│   └── analysis/
│       ├── trace.py               # Trace metrics (cost, latency, tool calls)
│       └── judge.py               # LLM-as-judge scoring of answers
├── pyproject.toml
└── README.md
```

## Setup

Requires Python ≥3.13 and [uv](https://docs.astral.sh/uv/):

```bash
uv sync
```

## Data Preparation

### 1. Download PDFs

```bash
uv run download-data --category real-pdfQA --dataset ClimateFinanceBench --yes
```

This fetches PDFs from the `pdfqa/pdfQA-Benchmark` Hugging Face dataset into `data/`.

### 2. Download Question-Answer Annotations

```bash
uv run download-qa --category real-pdfQA --dataset ClimateFinanceBench --yes
```

This fetches annotated Q&A JSONs from the `pdfqa/pdfQA-Annotations` dataset into `questions/`.

## Running the Benchmark

The `run_microag.py` script shells out to the [MicroAgents](https://github.com/AstraBert/microagents) runtime and captures its JSON-RPC event stream.

| Mode | Command | Description |
|------|---------|-------------|
| MicroAgents | `run-microag --show-prompt --subset 1-15` | Generates prompts from the dataset questions and runs them through the `microag` CLI |

A convenience script is provided in `scripts/`:

```bash
bash scripts/run_bench_microag.sh
```

### A note on the `microag` harness

[MicroAgents](https://github.com/AstraBert/microagents) is a small Rust agent runtime that exposes file-reading and search as native tools (`read`, `search`) and emits a JSON-RPC notification stream over stdout: `session.init`, `tool.call`, `tool.result`, `stream.delta`, `assistant.response`, `session.stop`, etc. `run_microag.py` spawns the `microag` binary per prompt (configured here against `anthropic/claude-sonnet-4.6` via OpenRouter), parses those events into typed dataclasses, writes one JSONL line per event into `traces/microag/`, and persists the final answer to `answers/microag/`.

Cost is not reported by the runtime, so the trace analyzer derives it from the `estimated_input_tokens` / `estimated_output_tokens` carried on the final `session.stop` event using OpenAI-style GPT pricing (cost might be underestimated).

Prerequisites to run the benchmark:

- `OPENROUTER_API_KEY` exported in the environment.

## Output

For each run, the benchmark produces:

- **`answers/microag/<document>.json`** — Structured answer with:
  - `question` — The original question
  - `file_path` — Path to the source PDF
  - `answer` — Claude's concise answer
  - `confidence` — Integer confidence score (0–100)
  - `reasoning` — Brief reasoning (≤500 characters)

- **`traces/microag/<document>.jsonl`** — Full interaction trace including tool calls, model outputs, token usage, and errors

## How It Works

1. **Prompt building** (`build_prompts`): Iterates over all question files in `questions/`, validates them against `QAResult`, and pairs each question with its corresponding PDF in `data/`.

2. **Execution**: Sends each prompt to the MicroAgents CLI, streams responses, and writes structured outputs and traces to disk.

## Evaluation

Two analyses are available:

### Trace metrics

```bash
uv run trace-analyzer     # cost / latency / tool-call stats derived from MicroAgents event traces
```

The analyzer in `analysis/trace.py` walks the JSON-RPC events itself — counting `tool.call` invocations by name (`read`, `search`), summing token usage from the `session.stop` event, and pricing it at Sonnet rates.

### LLM-as-judge

```bash
uv run judge-answers
```

`analysis/judge.py` scores each saved answer against the dataset's gold annotation and writes per-document `EvaluationSummary` JSONs to `evaluations/microag/`. Already-evaluated modes are loaded from disk on subsequent runs, so re-running only judges the modes that don't yet have an `evaluations/` directory.

## Latest Benchmark Results

### LLM-as-a-judge

> _Note: `gpt` -> `gpt-4.1`; `gemini` -> `gemini-3.5-flash`_

| File | Score |
|---|---|
| 2022 | 32.5 |
| 2022_1 | 97.5 |
| 2022_2 | 100.0 |
| Apple_Environmental_Progress_Report_2024 | 5.0 |
| Apple_Environmental_Progress_Report_2024_1 | 100.0 |
| Apple_Environmental_Progress_Report_2024_2 | 90.0 |
| Microsoft-2024-Environmental-Sustainability-Report | 55.0 |
| Microsoft-2024-Environmental-Sustainability-Report_1 | 100.0 |
| Microsoft-2024-Environmental-Sustainability-Report_2 | 100.0 |
| Orange 2023 IAR - On track | 5.0 |
| Orange 2023 IAR - On track_1 | 92.5 |
| Orange 2023 IAR - On track_2 | 95.0 |
| bp-sustainability-report-2023 | 15.0 |
| bp-sustainability-report-2023_1 | 30.0 |
| bp-sustainability-report-2023_2 | 72.5 |
| **AVG (all files)** | **66.00** |

*(Per-file Answer score, avg across judges)*

| File | Score |
|---|---|
| 2022 | 35.0 |
| 2022_1 | 97.5 |
| 2022_2 | 100.0 |
| Apple_Environmental_Progress_Report_2024 | 40.0 |
| Apple_Environmental_Progress_Report_2024_1 | 100.0 |
| Apple_Environmental_Progress_Report_2024_2 | 95.0 |
| Microsoft-2024-Environmental-Sustainability-Report | 65.0 |
| Microsoft-2024-Environmental-Sustainability-Report_1 | 97.5 |
| Microsoft-2024-Environmental-Sustainability-Report_2 | 100.0 |
| Orange 2023 IAR - On track | 25.0 |
| Orange 2023 IAR - On track_1 | 90.0 |
| Orange 2023 IAR - On track_2 | 97.5 |
| bp-sustainability-report-2023 | 25.0 |
| bp-sustainability-report-2023_1 | 45.0 |
| bp-sustainability-report-2023_2 | 82.5 |
| **AVG (all files)** | **73.00** |

*(Per-file Reasoning score, avg across judges)*

| Judge / Metric | Score |
|---|---|
| gemini answer | 62.67 |
| gemini reasoning | 67.67 |
| gpt answer | 69.33 |
| gpt reasoning | 78.33 |
| overall answer | 66.00 |
| overall reasoning | 73.00 |

*(Per-judge average evaluation, across all files)*

### Trace Metrics

| MicroAgent metric | value |
|---|---|
| n_traces | 15.0000 |
| success_rate (%) | 100.0000 |
| avg_estimated_cost ($) | 0.2623 |
| p95_estimated_cost ($) | 0.4959 |
| avg_estimated_input_tokens | 138460.8667 |
| avg_estimated_output_tokens | 1430.6667 |
| avg_latency (s) | 41.3561 |
| p95_latency (s) | 62.0856 |
| avg_tool_calls | 6.0000 |

| Tool name | # calls |
|---|---|
| search | 42 |
| shell_execute | 28 |
| read | 20 |
