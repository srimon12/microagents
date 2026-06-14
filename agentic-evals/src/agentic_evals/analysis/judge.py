import asyncio
import json
import os
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

import openrouter
from openrouter.components.chatcontenttext import ChatContentText
from openrouter.components.chatformatjsonschemaconfig import ChatFormatJSONSchemaConfig
from openrouter.components.chatjsonschemaconfig import ChatJSONSchemaConfig
from openrouter.components.chatrequest import ChatRequestReasoningTypedDict
from openrouter.components.chatusermessage import ChatUserMessage
from pydantic import BaseModel, Field
from rich.console import Console
from rich.table import Table

from ..run_microag import Answer, QAResult

JUDGE_MODELS = [
    "google/gemini-3.5-flash",
    "openai/gpt-4.1",
]
JUDGE_LABELS = ["gemini", "gpt"]


class Evaluation(BaseModel):
    answer_score: int = Field(
        description="Score for the model-provided answer, based on the ground truth answer. Must be between 0 (very poor performance) and 100 (excellent performance).",
        ge=0,
        le=100,
    )
    reasoning_score: int = Field(
        description="Score for the reasoning behind the model-provided answer. Must be between 0 (very poor performance) and 100 (excellent performance).",
        ge=0,
        le=100,
    )
    explanation: str = Field(
        description="Brief explanation of the evaluation given. Must not exceed 300 characters"
    )


class EvaluationSummary(BaseModel):
    gemini_eval: Evaluation
    gpt_eval: Evaluation


@dataclass
class SkillEvalSummary:
    variant: str
    file_names: list[str]
    # per-file averages (across judges)
    avg_answer_per_file: list[float]
    avg_reasoning_per_file: list[float]
    # per-judge averages (across files)
    gemini_avg_answer: float
    gemini_avg_reasoning: float
    gpt_avg_answer: float
    gpt_avg_reasoning: float
    # overall averages
    overall_avg_answer: float
    overall_avg_reasoning: float


async def evaluate_one(skill: str | None, file_name: str) -> EvaluationSummary:
    os.makedirs(Path.cwd() / "evaluations" / (skill or "raw"), exist_ok=True)
    file = (
        Path().cwd() / "answers" / (skill if skill else "raw") / (file_name + ".json")
    )
    original_file_name = file_name
    question_num = 0
    if file_name[-2] == "_" and file_name[-1].isdigit():
        question_num = int(file_name[-1])
        file_name = file_name[:-2]
    question_file = Path().cwd() / "questions" / (file_name + ".json")
    with open(file, "r") as f:
        content = f.read()
    answer = Answer.model_validate_json(content)
    with open(question_file, "r") as f:
        data = json.load(f)
        q = data[question_num]
    question = QAResult.model_validate(q)
    prompt = f"<question>{question.question}</question>\n<ground_truth>{question.answer}</ground_truth>\n<model_answer>{answer.answer}</model_answer>\n<model_reasoning>{answer.reasoning}</model_reasoning>\n<model_confidence>{answer.confidence}</model_confidence>\n<task>Evaluate the model answer and reasoning in relation with the question, the ground truth answer and the confidence the model had in the answer</task>"
    evals = []
    async with openrouter.OpenRouter(
        api_key=os.getenv("OPENROUTER_API_KEY", "")
    ) as client:
        for model in JUDGE_MODELS:
            print(model)
            response = await client.chat.send_async(
                messages=[ChatUserMessage(role="user", content=prompt)],
                response_format=ChatFormatJSONSchemaConfig(
                    json_schema=ChatJSONSchemaConfig(
                        name="evaluation",
                        description="Evaluation form for LLM-as-a-judge tasks on QA",
                        schema_=Evaluation.model_json_schema(),
                    ),
                    type="json_schema",
                ),
                model=model,
                reasoning=ChatRequestReasoningTypedDict(effort="low"),
            )
            content = response.choices[0].message.content
            if isinstance(content, str):
                evaluation = Evaluation.model_validate_json(content)
            elif isinstance(content, list):
                text = ""
                for c in content:
                    if isinstance(c, ChatContentText):
                        text += c.text
                evaluation = Evaluation.model_validate_json(text)
            else:
                raise RuntimeError("Model did not produce an output in its response")
            evals.append(evaluation)
    summary = EvaluationSummary(gemini_eval=evals[0], gpt_eval=evals[1])
    ev_file = (
        Path.cwd() / "evaluations" / (skill or "raw") / (original_file_name + ".json")
    )
    with open(
        ev_file,
        "w",
    ) as f:
        print(f"Evaluation being recorded at {str(ev_file)}")
        f.write(summary.model_dump_json(indent=2))
    return summary


async def evaluate_skill(skill: str | None) -> dict[str, EvaluationSummary]:
    evaluations: dict[str, EvaluationSummary] = {}
    i = 0
    for f in os.listdir(f"answers/{skill or 'raw'}"):
        if os.path.isdir(os.path.join(f"answers/{skill or 'raw'}", f)):
            continue
        name = f.removesuffix(".json")
        i += 1
        print(f"Evaluating file {i} for {skill or 'raw'}")
        ev = await evaluate_one(skill, name)
        evaluations[name] = ev
    return evaluations


def summarize_skill_evaluations(
    variant: str | None, evaluations: dict[str, EvaluationSummary]
) -> SkillEvalSummary:
    file_names = sorted(evaluations.keys())

    avg_answer_per_file: list[float] = []
    avg_reasoning_per_file: list[float] = []

    judge_answer_scores: dict[str, list[float]] = defaultdict(list)
    judge_reasoning_scores: dict[str, list[float]] = defaultdict(list)

    for name in file_names:
        ev = evaluations[name]
        scores = [ev.gemini_eval, ev.gpt_eval]
        avg_answer = sum(e.answer_score for e in scores) / len(scores)
        avg_reasoning = sum(e.reasoning_score for e in scores) / len(scores)
        avg_answer_per_file.append(avg_answer)
        avg_reasoning_per_file.append(avg_reasoning)

        for label, e in zip(JUDGE_LABELS, scores):
            judge_answer_scores[label].append(float(e.answer_score))
            judge_reasoning_scores[label].append(float(e.reasoning_score))

    def avg(values: list[float]) -> float:
        return sum(values) / len(values) if values else 0.0

    all_answer_scores = [s for scores in judge_answer_scores.values() for s in scores]
    all_reasoning_scores = [
        s for scores in judge_reasoning_scores.values() for s in scores
    ]

    return SkillEvalSummary(
        variant=variant if variant is not None else "raw",
        file_names=file_names,
        avg_answer_per_file=avg_answer_per_file,
        avg_reasoning_per_file=avg_reasoning_per_file,
        gemini_avg_answer=avg(judge_answer_scores["gemini"]),
        gemini_avg_reasoning=avg(judge_reasoning_scores["gemini"]),
        gpt_avg_answer=avg(judge_answer_scores["gpt"]),
        gpt_avg_reasoning=avg(judge_reasoning_scores["gpt"]),
        overall_avg_answer=avg(all_answer_scores),
        overall_avg_reasoning=avg(all_reasoning_scores),
    )


def _render_score_table(
    console: Console,
    title: str,
    row_label: str,
    row_labels: list[str],
    summaries: list[SkillEvalSummary],
    values_per_row: list[list[float | None]],
    footer_values: list[float],
) -> None:
    """Render a single rich Table with one column per variant.

    For each row, the best variant value is highlighted in bold green and
    annotated with a trophy. Missing values render as a dim em-dash.
    """
    table = Table(title=title, title_style="bold cyan", header_style="bold")
    table.add_column(row_label, style="bold", no_wrap=True)
    for s in summaries:
        table.add_column(s.variant, justify="right")

    for label, values in zip(row_labels, values_per_row):
        present = [v for v in values if v is not None]
        best = max(present) if present else None
        cells: list[str] = [label]
        for v in values:
            if v is None:
                cells.append("[dim]—[/dim]")
            elif best is not None and v == best and len(present) > 1:
                cells.append(f"[bold green]{v:.1f} 🏆[/bold green]")
            else:
                cells.append(f"{v:.1f}")
        table.add_row(*cells)

    # Footer row (per-variant average across all files), highlighted similarly
    best_footer = max(footer_values) if footer_values else None
    footer_cells: list[str] = ["[italic]AVG (all files)[/italic]"]
    for v in footer_values:
        if best_footer is not None and v == best_footer and len(footer_values) > 1:
            footer_cells.append(f"[bold green]{v:.2f} 🏆[/bold green]")
        else:
            footer_cells.append(f"[italic]{v:.2f}[/italic]")
    table.add_section()
    table.add_row(*footer_cells)

    console.print(table)


def _per_file_values(
    summaries: list[SkillEvalSummary],
    all_files: list[str],
    field: str,
) -> list[list[float | None]]:
    """Build a matrix [file][variant] -> score for the given per-file field."""
    rows: list[list[float | None]] = []
    for file in all_files:
        row: list[float | None] = []
        for s in summaries:
            if file in s.file_names:
                idx = s.file_names.index(file)
                row.append(getattr(s, field)[idx])
            else:
                row.append(None)
        rows.append(row)
    return rows


def print_eval_summary(summaries: list[SkillEvalSummary]) -> None:
    console = Console()
    all_files = sorted({f for s in summaries for f in s.file_names})

    # ---- Per-file Answer scores ----
    answer_rows = _per_file_values(summaries, all_files, "avg_answer_per_file")
    _render_score_table(
        console,
        title="Per-file Answer score (avg across judges)",
        row_label="File",
        row_labels=all_files,
        summaries=summaries,
        values_per_row=answer_rows,
        footer_values=[s.overall_avg_answer for s in summaries],
    )
    console.print()

    # ---- Per-file Reasoning scores ----
    reasoning_rows = _per_file_values(summaries, all_files, "avg_reasoning_per_file")
    _render_score_table(
        console,
        title="Per-file Reasoning score (avg across judges)",
        row_label="File",
        row_labels=all_files,
        summaries=summaries,
        values_per_row=reasoning_rows,
        footer_values=[s.overall_avg_reasoning for s in summaries],
    )
    console.print()

    # ---- Per-judge averages across all files ----
    judge_table = Table(
        title="Per-judge average evaluation (across all files)",
        title_style="bold cyan",
        header_style="bold",
    )
    judge_table.add_column("Judge / Metric", style="bold", no_wrap=True)
    for s in summaries:
        judge_table.add_column(s.variant, justify="right")

    judge_rows: list[tuple[str, list[float]]] = [
        ("gemini answer", [s.gemini_avg_answer for s in summaries]),
        ("gemini reasoning", [s.gemini_avg_reasoning for s in summaries]),
        ("gpt answer", [s.gpt_avg_answer for s in summaries]),
        ("gpt reasoning", [s.gpt_avg_reasoning for s in summaries]),
        ("overall answer", [s.overall_avg_answer for s in summaries]),
        ("overall reasoning", [s.overall_avg_reasoning for s in summaries]),
    ]
    for label, values in judge_rows:
        best = max(values) if values else None
        is_overall = label.startswith("overall")
        cells: list[str] = [f"[italic]{label}[/italic]" if is_overall else label]
        for v in values:
            if best is not None and v == best and len(values) > 1:
                cells.append(f"[bold green]{v:.2f} 🏆[/bold green]")
            elif is_overall:
                cells.append(f"[italic]{v:.2f}[/italic]")
            else:
                cells.append(f"{v:.2f}")
        if label == "stepfun reasoning":
            # visual break before the overall block
            judge_table.add_row(*cells)
            judge_table.add_section()
        else:
            judge_table.add_row(*cells)

    console.print(judge_table)
    console.print()


async def judge() -> None:
    if not os.path.exists("evaluations/microag"):
        microag_evals = await evaluate_skill("microag")
    else:
        print("Loading microag evaluations from existing files...")
        microag_evals: dict[str, EvaluationSummary] = {}
        for file in os.listdir("evaluations/microag"):
            with open(
                os.path.join("evaluations", "microag", file),
                "r",
            ) as f:
                content = f.read()
                summary = EvaluationSummary.model_validate_json(content)
                microag_evals.update({file.removesuffix(".json"): summary})

    summaries = [
        summarize_skill_evaluations("microag", microag_evals),
    ]

    print_eval_summary(summaries)


def main() -> None:
    asyncio.run(judge())
