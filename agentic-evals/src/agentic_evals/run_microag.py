import asyncio
import json
import os
import shlex
import sys
from argparse import ArgumentParser
from asyncio.subprocess import Process
from dataclasses import asdict, dataclass
from datetime import datetime
from enum import Enum
from pathlib import Path
from typing import Any

from pydantic import BaseModel, Field

STREAM_LIMIT = 100 * 1024 * 1024  # 100MB
MICROAG_EXECUTABLE = "../target/release/microag"

# --- Helpers ---


class QAResult(BaseModel):
    question: str = Field(description="Question asked")
    answer: str = Field(description="Generated answer")
    sources: list[str] = Field(
        default_factory=list,
        description="Source passages used to answer the question",
    )
    num_sources_used: int = Field(description="Number of sources used")
    file_name: str = Field(description="Source document name")


class Answer(BaseModel):
    question: str = Field(
        description="Question that was asked by the user, reported verbatim"
    )
    file_path: str = Field(description="Path to the file the question was referring to")
    answer: str = Field(
        description="Concise answer to the user's question, briefly referncing passages in the source document supporting it."
    )
    confidence: int = Field(
        description="Confidence score, must be an integer between 0 and 100",
        ge=0,
        le=100,
    )
    reasoning: str = Field(
        description="Brief reasoning explaining the answer. Must not exceed 500 characters."
    )


def build_prompts() -> list[tuple[str, str]]:
    prompts: list[tuple[str, str]] = []

    questions_dir = Path.cwd() / "questions"
    data_dir = Path.cwd() / "data"
    question_files = [questions_dir / f for f in os.listdir(questions_dir)]
    for qf in question_files:
        with open(qf, "r") as fp:
            data = json.load(fp)
            for d in data:
                mod = QAResult.model_validate(d)
                pdf_file = data_dir / (mod.file_name + ".pdf")
                assert pdf_file.exists(), f"PDF file {str(pdf_file)} does not exist"
                prompts.append(
                    (
                        str(pdf_file),
                        f"Process {str(pdf_file)} using the builtin tool available to you for PDF reading, then answer the following question:\n\n```text\n{mod.question}\n```\n\nThe answer should be concise and briefly reference the essential passages supporting it. If you cannot find sufficient information to answer the question, your answer should be: 'Not available in the retrieved information'.",
                    )
                )
    return prompts


def find_free_path(path: Path) -> Path:
    if not path.exists():
        return path
    n = 1
    while (candidate := path.with_stem(f"{path.stem}_{n}")).exists():
        n += 1
    return candidate


# --- Event types matching your Rust definitions ---


class SessionInitType(str, Enum):
    Start = "Start"
    Resume = "Resume"


class DeltaType(str, Enum):
    Text = "Text"
    Thinking = "Thinking"


@dataclass
class Usage:
    latency: int
    input_chars: int
    estimated_input_tokens: int
    output_chars: int
    estimated_output_tokens: int


@dataclass
class SessionInitEvent:
    session_id: str
    model: str
    provider: str
    system: str
    init_type: SessionInitType
    timestamp: float


@dataclass
class SessionStopEvent:
    session_id: str
    success: bool
    timestamp: float
    usage: Usage
    result: str | None = None
    error: str | None = None


@dataclass
class UserPromptSubmitEvent:
    session_id: str
    turn_id: str
    prompt: str
    timestamp: float


@dataclass
class StreamDeltaEvent:
    session_id: str
    turn_id: str
    delta: str
    delta_type: DeltaType
    timestamp: float


@dataclass
class ToolCallEvent:
    session_id: str
    turn_id: str
    name: str
    input: Any
    timestamp: float


@dataclass
class ToolResultEvent:
    session_id: str
    turn_id: str
    result: Any
    tool_call_id: str
    timestamp: float


@dataclass
class SkillLoadEvent:
    session_id: str
    turn_id: str
    skill_name: str
    timestamp: float


@dataclass
class AssistantResponseEvent:
    session_id: str
    turn_id: str
    full_text: str
    timestamp: float
    tool_calls: list[Any] | None = None


AgentEvent = (
    SessionInitEvent
    | SessionStopEvent
    | UserPromptSubmitEvent
    | StreamDeltaEvent
    | ToolCallEvent
    | ToolResultEvent
    | SkillLoadEvent
    | AssistantResponseEvent
)


# --- Parser ---


def parse_timestamp(ts: str) -> float:
    return datetime.fromisoformat(ts.replace("Z", "+00:00")).timestamp()


def parse_event(method: str, params: dict) -> AgentEvent | None:
    ts = parse_timestamp(params["timestamp"])

    match method:
        case "session.init":
            return SessionInitEvent(
                session_id=params["session_id"],
                model=params["model"],
                provider=params["provider"],
                system=params["system"],
                init_type=SessionInitType(params["init_type"]),
                timestamp=ts,
            )
        case "session.stop":
            u = params["usage"]
            return SessionStopEvent(
                session_id=params["session_id"],
                success=params["success"],
                result=params.get("result"),
                error=params.get("error"),
                timestamp=ts,
                usage=Usage(**u),
            )
        case "user.prompt.submit":
            return UserPromptSubmitEvent(
                session_id=params["session_id"],
                turn_id=params["turn_id"],
                prompt=params["prompt"],
                timestamp=ts,
            )
        case "stream.delta":
            return StreamDeltaEvent(
                session_id=params["session_id"],
                turn_id=params["turn_id"],
                delta=params["delta"],
                delta_type=DeltaType(params["delta_type"]),
                timestamp=ts,
            )
        case "tool.call":
            return ToolCallEvent(
                session_id=params["session_id"],
                turn_id=params["turn_id"],
                name=params["name"],
                input=params["input"],
                timestamp=ts,
            )
        case "tool.result":
            return ToolResultEvent(
                session_id=params["session_id"],
                turn_id=params["turn_id"],
                result=params["result"],
                tool_call_id=params["tool_call_id"],
                timestamp=ts,
            )
        case "skill.load":
            return SkillLoadEvent(
                session_id=params["session_id"],
                turn_id=params["turn_id"],
                skill_name=params["skill_name"],
                timestamp=ts,
            )
        case "assistant.response":
            return AssistantResponseEvent(
                session_id=params["session_id"],
                turn_id=params["turn_id"],
                full_text=params["full_text"],
                tool_calls=params.get("tool_calls"),
                timestamp=ts,
            )
        case _:
            print(f"[warn] unknown method: {method}")
            return None


# --- Process runner ---


async def run_agent(prompt: str, trace_file: str, verbose: bool = False) -> str | None:
    env = os.environ.copy()
    proc: Process = await asyncio.create_subprocess_exec(
        *[
            MICROAG_EXECUTABLE,
            "--provider",
            "openrouter",
            "--model",
            "openai/gpt-5.3-codex",
            "--prompt",
            shlex.quote(prompt),
        ],
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=env,
        limit=STREAM_LIMIT,
    )

    if verbose:
        print(f"Agent started (pid={proc.pid})")

    try:
        assert proc.stdout is not None, "Standard output stream is none"
        async for raw_line in proc.stdout:
            line = raw_line.decode().strip()
            if not line:
                continue

            try:
                msg = json.loads(line)
            except json.JSONDecodeError as e:
                print(
                    f"[warn] failed to parse line: {e}\n  raw: {line}", file=sys.stderr
                )
                continue

            # Ignore JSON-RPC responses (have 'id' + 'result'/'error'),
            # we only care about notifications here
            if "method" not in msg:
                continue

            event = parse_event(msg["method"], msg.get("params", {}))
            if event is None:
                continue

            with open(trace_file, "a") as f:
                f.write(json.dumps(asdict(event)) + "\n")
            # Dispatch
            match event:
                case SessionInitEvent():
                    if verbose:
                        print(
                            f"[session.init] id={event.session_id} model={event.model}"
                        )
                case SessionStopEvent(success=True):
                    if verbose:
                        print(f"[session.stop] done. result={event.result}")
                    return event.result  # natural exit — agent finished
                case SessionStopEvent(success=False):
                    if verbose:
                        print(f"[session.stop] failed: {event.error}")
                    break
                case StreamDeltaEvent():
                    if verbose:
                        print(event.delta, end="", flush=True)
                case ToolCallEvent():
                    if verbose:
                        print(f"\n[tool.call] {event.name}({event.input})")
                case ToolResultEvent():
                    if verbose:
                        print(
                            f"[tool.result] {event.tool_call_id}: {str(event.result)[:100]}"
                        )
                case AssistantResponseEvent():
                    if verbose:
                        print(f"\n[assistant.response] turn={event.turn_id}")
                case _:
                    pass
    except asyncio.CancelledError:
        pass
    finally:
        if proc.returncode is None:
            proc.terminate()
            try:
                await asyncio.wait_for(proc.wait(), timeout=5.0)
            except asyncio.TimeoutError:
                proc.kill()

        print(f"Agent exited (code={proc.returncode})")


async def run_on_prompts(verbose: bool, subset: str | None, show_prompt: bool) -> None:
    prompts = build_prompts()
    if subset is not None:
        if "-" not in subset:
            prompts = [prompts[int(subset) - 1]]
        elif len(subset) >= 3 and "-" in subset:
            split = subset.split("-")
            start, end = int(split[0].strip()) - 1, int(split[1].strip())
            prompts = prompts[start:end]
        else:
            print("Invalid subset, considering all prompts")
    counter = 0
    os.makedirs(Path.cwd() / "traces" / "microag", exist_ok=True)
    os.makedirs(Path.cwd() / "answers" / "microag", exist_ok=True)
    for fl, prompt in prompts:
        print(f"Running prompt: {counter + 1}/{len(prompts)}")
        prompt += f"The final answer should follow this JSON schema: {json.dumps(Answer.model_json_schema(), indent=2)}. Provide the answer in plain JSON, with no markdown syntax annotation."
        if show_prompt:
            print(f"\nPrompt:\n{prompt}\n")
        trace_file = find_free_path(
            Path.cwd() / "traces" / "microag" / (Path(fl).stem + ".jsonl")
        )
        answer_file = find_free_path(
            Path.cwd() / "answers" / "microag" / (Path(fl).stem + ".raw.txt")
        )
        trace_file.touch()
        answer_file.touch()
        answer = await run_agent(prompt, str(trace_file), verbose)
        if answer:
            print(f"Done with prompt: {counter + 1}/{len(prompts)} (SUCCESS)")
            with open(answer_file, "w") as f:
                f.write(answer)
        else:
            print(f"Done with prompt: {counter + 1}/{len(prompts)} (FAILED)")
        counter += 1


def main() -> None:
    parser = ArgumentParser()

    parser.add_argument(
        "--show-prompt",
        help="Show the prompt that you are about to run",
        required=False,
        default=False,
        action="store_true",
    )
    parser.add_argument(
        "--subset",
        help="If a number is provide, the corresponding prompt will be run. If a range (n-m, where n and m are integers) is provided, the prompts within that range will be run.",
        required=False,
        default=None,
    )
    parser.add_argument(
        "--verbose",
        help="Wheter to print the agent's output on console",
        required=False,
        action="store_true",
        default=False,
    )
    args = parser.parse_args()
    asyncio.run(run_on_prompts(args.verbose, args.subset, args.show_prompt))
