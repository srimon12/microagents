import json
import os
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import cast

import numpy as np

# ---------------------------------------------------------------------------
# MicroAgent trace analysis
#
# Each line is one event, distinguished by its keys:
#   - init:        {init_type, model, provider, system}
#   - prompt:      {prompt}
#   - tool call:   {name, input}
#   - tool result: {result, tool_call_id}
#   - text delta:  {delta, delta_type}                  (streamed, ignored)
#   - turn end:    {full_text, tool_calls}
#   - final:       {success, usage, result, error}      (usage carries tokens)
#
# These traces do not invoke skills or the `lit` CLI; the agent reads/searches
# documents with the builtin `read` and `search` tools. Cost is not reported by
# the runtime, so we derive it from the estimated token counts in `usage`.
# ---------------------------------------------------------------------------

# Anthropic-style pricing applied to MicroAgent token usage.
MICROAG_INPUT_COST_PER_TOKEN = 1.75 / 1_000_000
MICROAG_OUTPUT_COST_PER_TOKEN = 14.0 / 1_000_000


@dataclass
class MicroAgUsage:
    input_tokens: int
    output_tokens: int
    latency_ms: float

    @property
    def cost(self) -> float:
        return (
            self.input_tokens * MICROAG_INPUT_COST_PER_TOKEN
            + self.output_tokens * MICROAG_OUTPUT_COST_PER_TOKEN
        )


@dataclass
class MicroAgTraceSummary:
    file_name: str
    model: str | None
    success: bool
    error: str | None
    tool_calls: list[str]  # tool names in call order
    usage: MicroAgUsage | None

    @property
    def read_calls(self) -> int:
        return sum(1 for t in self.tool_calls if t == "read")

    @property
    def search_calls(self) -> int:
        return sum(1 for t in self.tool_calls if t == "search")

    @property
    def cost(self) -> float | None:
        return self.usage.cost if self.usage is not None else None


def process_one_microag_trace(path: Path) -> MicroAgTraceSummary:
    model: str | None = None
    success = False
    error: str | None = None
    tool_calls: list[str] = []
    usage: MicroAgUsage | None = None

    with open(path, "r") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            d = json.loads(line)
            if "init_type" in d:
                model = d.get("model")
            elif "name" in d and "input" in d:
                tool_calls.append(d["name"])
            elif "usage" in d and d.get("usage"):
                u = d["usage"]
                usage = MicroAgUsage(
                    input_tokens=int(u.get("estimated_input_tokens", 0)),
                    output_tokens=int(u.get("estimated_output_tokens", 0)),
                    latency_ms=float(u.get("latency", 0.0)),
                )
                success = bool(d.get("success"))
                error = d.get("error")

    return MicroAgTraceSummary(
        file_name=path.stem,
        model=model,
        success=success,
        error=error,
        tool_calls=tool_calls,
        usage=usage,
    )


@dataclass
class MicroAgBenchSummary:
    n_traces: int
    success_rate: float  # fraction of traces that succeeded
    avg_cost: float
    p95_cost: float
    avg_input_tokens: float
    avg_output_tokens: float
    avg_latency_s: float
    p95_latency_s: float
    avg_tool_calls: float
    tool_call_counts: dict[str, int]


def process_microag_traces(directory: str = "traces/microag") -> MicroAgBenchSummary:
    summaries = [
        process_one_microag_trace(Path(directory) / f)
        for f in os.listdir(directory)
        if f.endswith(".jsonl")
    ]

    def p95(values: list[float]) -> float:
        return float(np.percentile(values, 95)) if values else 0.0

    def avg(values: list[float]) -> float:
        return sum(values) / len(values) if values else 0.0

    with_usage = [s for s in summaries if s.usage is not None]
    costs = [s.cost for s in with_usage if s.cost is not None]
    latencies = [cast(MicroAgUsage, s.usage).latency_ms / 1000.0 for s in with_usage]

    tool_call_counts: Counter[str] = Counter()
    for s in summaries:
        tool_call_counts.update(s.tool_calls)

    return MicroAgBenchSummary(
        n_traces=len(summaries),
        success_rate=avg([float(s.success) for s in summaries]),
        avg_cost=avg(costs),
        p95_cost=p95(costs),
        avg_input_tokens=avg(
            [float(cast(MicroAgUsage, s.usage).input_tokens) for s in with_usage]
        ),
        avg_output_tokens=avg(
            [float(cast(MicroAgUsage, s.usage).output_tokens) for s in with_usage]
        ),
        avg_latency_s=avg(latencies),
        p95_latency_s=p95(latencies),
        avg_tool_calls=avg([float(len(s.tool_calls)) for s in summaries]),
        tool_call_counts=dict(tool_call_counts),
    )


def print_microag_summary(directory: str = "traces/microag") -> None:
    b = process_microag_traces(directory)
    rows = [
        ("n_traces", b.n_traces),
        ("success_rate (%)", b.success_rate * 100.0),
        ("avg_cost ($)", b.avg_cost),
        ("p95_cost ($)", b.p95_cost),
        ("avg_input_tokens", b.avg_input_tokens),
        ("avg_output_tokens", b.avg_output_tokens),
        ("avg_latency (s)", b.avg_latency_s),
        ("p95_latency (s)", b.p95_latency_s),
        ("avg_tool_calls", b.avg_tool_calls),
    ]
    print(f"{'MicroAgent metric':<24}{'value':>16}")
    print("-" * 40)
    for label, value in rows:
        print(f"{label:<24}{value:>16.4f}")

    print()
    print(f"{'Tool name':<24}{'# calls':>16}")
    print("-" * 40)
    for name, count in sorted(b.tool_call_counts.items(), key=lambda x: -x[1]):
        print(f"{name:<24}{count:>16d}")


def main() -> None:
    print_microag_summary()
