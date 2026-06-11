use std::fs;
use std::{collections::HashSet, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::load_questions::{Question, RelevantChunk};

/// A single search result entry from the raw results JSON.
#[derive(Debug, Deserialize)]
pub struct RawResult {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    score: f64,
    payload: ResultPayload,
    #[allow(dead_code)]
    order_value: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ResultPayload {
    document_path: String,
    #[serde(default)]
    line_start: Option<u32>,
    #[serde(default)]
    line_end: Option<u32>,
    #[allow(dead_code)]
    content: Option<String>,
}

/// Per-question comparison metrics.
#[derive(Debug, Serialize)]
pub struct QuestionMetrics {
    pub query: String,
    pub total_relevant: usize,
    pub total_retrieved: usize,
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub mrr: f64,
    /// Indices (1-based) in the retrieved results where a relevant chunk was found.
    pub relevant_ranks: Vec<usize>,
    /// Chunks that were retrieved
    pub retrieved_chunks: Vec<String>,
    /// Which relevant chunks were not found in the retrieved results.
    pub missed_chunks: Vec<String>,
}

/// Overall experiment metrics.
#[derive(Debug, Serialize)]
pub struct ExperimentMetrics {
    pub experiment_name: String,
    pub num_questions: usize,
    pub avg_precision: f64,
    pub avg_recall: f64,
    pub avg_f1: f64,
    pub avg_mrr: f64,
    pub questions: Vec<QuestionMetrics>,
}

/// Check whether a retrieved result matches a relevant chunk.
///
/// A match is defined as:
/// - The file paths match (after normalising), **and**
/// - The line ranges overlap (if line numbers are present in both).
fn matches_chunk(result: &ResultPayload, chunk: &RelevantChunk) -> bool {
    let result_path = normalise_path(&result.document_path);
    let chunk_path = normalise_path(&chunk.file);

    if result_path != chunk_path {
        return false;
    }

    // If either side is missing line numbers, we consider it a match on path alone.
    let Some(r_start) = result.line_start else {
        return true;
    };
    let Some(r_end) = result.line_end else {
        return true;
    };

    // Overlap: [a, b] overlaps with [c, d] iff a <= d && c <= b

    r_start <= chunk.line_end && chunk.line_start <= r_end
}

fn normalise_path(p: &str) -> String {
    let p = p.replace('\\', "/");
    let path = Path::new(&p)
        .canonicalize()
        .expect("Should be able to canonicalize path");
    path.to_string_lossy().to_string()
}

/// Compare the retrieved results for one question against its relevant chunks.
pub fn compare_question(question: &Question, raw_results: &[RawResult]) -> QuestionMetrics {
    let total_relevant = question.relevant_chunks.len();
    let total_retrieved = raw_results.len();

    let mut true_positives = 0usize;
    let mut relevant_ranks = Vec::new();
    let mut found_chunks = HashSet::new();

    for (rank0, result) in raw_results.iter().enumerate() {
        let rank = rank0 + 1; // 1-based
        for (idx, chunk) in question.relevant_chunks.iter().enumerate() {
            if found_chunks.contains(&idx) {
                continue;
            }
            if matches_chunk(&result.payload, chunk) {
                true_positives += 1;
                found_chunks.insert(idx);
                relevant_ranks.push(rank);
                break; // each result can only match one chunk
            }
        }
    }

    let false_positives = total_retrieved.saturating_sub(true_positives);
    let false_negatives = total_relevant.saturating_sub(true_positives);

    let precision = if total_retrieved > 0 {
        true_positives as f64 / total_retrieved as f64
    } else {
        0.0
    };
    let recall = if total_relevant > 0 {
        true_positives as f64 / total_relevant as f64
    } else {
        0.0
    };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    // MRR: 1 / rank of first relevant result, or 0 if none found.
    let mrr = relevant_ranks
        .first()
        .map(|&r| 1.0 / r as f64)
        .unwrap_or(0.0);

    let missed_chunks: Vec<String> = question
        .relevant_chunks
        .iter()
        .enumerate()
        .filter(|(i, _)| !found_chunks.contains(i))
        .map(|(_, c)| {
            format!(
                "{}:{}-{}",
                normalise_path(&c.file),
                c.line_start,
                c.line_end
            )
        })
        .collect();

    let retrieved_chunks: Vec<String> = raw_results
        .iter()
        .map(|r| {
            format!(
                "{}:{}-{}",
                r.payload.document_path,
                r.payload.line_start.unwrap_or_default(),
                r.payload.line_end.unwrap_or_default()
            )
        })
        .collect();

    QuestionMetrics {
        query: question.query.clone(),
        total_relevant,
        total_retrieved,
        true_positives,
        false_positives,
        false_negatives,
        precision,
        recall,
        f1,
        mrr,
        relevant_ranks,
        retrieved_chunks,
        missed_chunks,
    }
}

/// Load the raw results JSON for a single question.
fn load_results(path: &str) -> anyhow::Result<Vec<RawResult>> {
    let content = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&content)?;
    let raw = value
        .get("raw")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing 'raw' array in {}", path))?;
    let results: Vec<RawResult> = raw
        .iter()
        .map(|v| serde_json::from_value(v.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(results)
}

/// Run comparison for an entire experiment.
pub fn compare_experiment(
    experiment_name: &str,
    questions: &[Question],
) -> anyhow::Result<ExperimentMetrics> {
    let mut question_metrics = Vec::with_capacity(questions.len());

    for (i, question) in questions.iter().enumerate() {
        let result_path = format!("evals/experiment-{}/{}.json", experiment_name, i + 1);
        let raw_results = load_results(&result_path)?;
        let metrics = compare_question(question, &raw_results);
        question_metrics.push(metrics);
    }

    let num_questions = question_metrics.len();
    let avg_precision = if num_questions > 0 {
        question_metrics.iter().map(|m| m.precision).sum::<f64>() / num_questions as f64
    } else {
        0.0
    };
    let avg_recall = if num_questions > 0 {
        question_metrics.iter().map(|m| m.recall).sum::<f64>() / num_questions as f64
    } else {
        0.0
    };
    let avg_f1 = if num_questions > 0 {
        question_metrics.iter().map(|m| m.f1).sum::<f64>() / num_questions as f64
    } else {
        0.0
    };
    let avg_mrr = if num_questions > 0 {
        question_metrics.iter().map(|m| m.mrr).sum::<f64>() / num_questions as f64
    } else {
        0.0
    };

    Ok(ExperimentMetrics {
        experiment_name: experiment_name.to_string(),
        num_questions,
        avg_precision,
        avg_recall,
        avg_f1,
        avg_mrr,
        questions: question_metrics,
    })
}
