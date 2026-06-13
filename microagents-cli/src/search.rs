use fastembed::{RerankInitOptions, TextRerank};
use qdrant_edge::{
    AnyVariants, Condition, EdgeShard, FieldCondition, Filter, Fusion, JsonPath, Match, NamedQuery,
    Payload, Prefetch, QueryEnum, QueryRequest, ScoredPoint, ScoringQuery, ScrollRequest,
    SparseVector, WithPayloadInterface, WithVector, external::ordered_float::OrderedFloat,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::{Mutex, OnceLock},
};

use crate::{
    init_env::{
        CODE_VECTORS_NAME, EmbeddingPayload, SPARSE_VECTORS_NAME, VECTORS_NAME, load_qdrant_edge,
    },
    processing::fastembed_cache_dir,
};

pub const RERANK_TOP_N: usize = 5;
pub const DENSE_WEIGHT: f32 = 0.4;
pub const SPARSE_WEIGHT: f32 = 0.3;
pub const CODE_WEIGHT: f32 = 0.3;
pub const PREFETCH_TOP_N: usize = 30;
pub const RERANKER_LIMIT: usize = 100;
pub const TOP_K_FILES: usize = 5;
pub const TOP_CHUNKS_PER_FILE: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultWithScore {
    pub content: String,
    pub document_path: String,
    pub score: f32,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SearchResults {
    pub raw: Vec<ScoredPoint>,
    pub processed: Vec<ResultWithScore>,
}

static RERANKER_MODEL: OnceLock<Mutex<TextRerank>> = OnceLock::new();

fn reranker() -> &'static Mutex<TextRerank> {
    RERANKER_MODEL.get_or_init(|| {
        Mutex::new(
            TextRerank::try_new(
                RerankInitOptions::new(fastembed::RerankerModel::JINARerankerV1TurboEn)
                    .with_cache_dir(fastembed_cache_dir().to_owned())
                    .with_show_download_progress(true)
                    .with_intra_threads(2),
            )
            .expect("Should be able to initialize reranker"),
        )
    })
}

/// Convert Qdrant Payload back to DocMeta
fn payload_to_struct(payload: &Payload) -> Result<EmbeddingPayload, Box<dyn std::error::Error>> {
    let ep: EmbeddingPayload = serde_json::from_value(serde_json::to_value(payload)?)?;
    Ok(ep)
}

fn find_files(results: &[ScoredPoint]) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let mut files = HashSet::new();
    for r in results {
        let payload = match r.payload.as_ref() {
            Some(p) => p,
            None => continue,
        };
        let p = payload_to_struct(payload)?;
        files.insert(p.document_path);
    }
    Ok(files)
}

fn rerank(
    edge: &EdgeShard,
    original_query: &str,
    results: Vec<ScoredPoint>,
    top_k_files: usize,
    top_chunks_per_file: usize,
) -> Result<Vec<ResultWithScore>, Box<dyn std::error::Error>> {
    let files = find_files(&results)?;

    if files.is_empty() {
        return Ok(vec![]);
    }

    let flt = Some(Filter::new_must(Condition::Field(
        FieldCondition::new_match(
            JsonPath::from_str("document_path").map_err(|_| "invalid json path")?,
            Match::from(AnyVariants::Strings(files.iter().cloned().collect())),
        ),
    )));

    let (records, _) = edge.scroll(ScrollRequest {
        offset: None,
        limit: Some(RERANKER_LIMIT),
        filter: flt,
        with_payload: Some(WithPayloadInterface::Bool(true)),
        with_vector: WithVector::Bool(false),
        order_by: None,
    })?;

    let docs: Vec<EmbeddingPayload> = records
        .into_iter()
        .filter_map(|rec| rec.payload.as_ref().and_then(|p| payload_to_struct(p).ok()))
        .collect();

    if docs.is_empty() {
        return Ok(vec![]);
    }

    let contents: Vec<&str> = docs.iter().map(|e| e.content.as_str()).collect();

    let reranker_mu = reranker();
    let mut reranker = reranker_mu.lock()?;
    let reranked = reranker.rerank(original_query, contents, false, None)?;
    drop(reranker); // release lock as early as possible

    // Group chunks by file, keeping only the top `top_chunks_per_file` per file.
    // reranked comes back sorted by score descending already.
    let mut result_per_file: HashMap<String, Vec<ResultWithScore>> = HashMap::new();
    for rer in &reranked {
        let emb_pay = &docs[rer.index];
        let entry = result_per_file
            .entry(emb_pay.document_path.clone())
            .or_default();
        if entry.len() < top_chunks_per_file {
            entry.push(ResultWithScore {
                content: emb_pay.content.clone(),
                document_path: emb_pay.document_path.clone(),
                score: rer.score,
                line_start: emb_pay.line_start,
                line_end: emb_pay.line_end,
            });
        }
    }

    // Rank files by the max score of their best chunk, take top_k_files,
    // then flatten preserving within-file chunk order (already score-sorted).
    let mut file_scores: Vec<(String, f32)> = result_per_file
        .iter()
        .map(|(path, chunks)| {
            let max_score = chunks
                .iter()
                .map(|c| c.score)
                .fold(f32::NEG_INFINITY, f32::max);
            (path.clone(), max_score)
        })
        .collect();
    file_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let final_results = file_scores
        .into_iter()
        .take(top_k_files)
        .filter_map(|(path, _)| result_per_file.remove(&path))
        .flatten()
        .collect();

    Ok(final_results)
}

pub fn search(
    original_query: &str,
    query_embedding: Vec<f32>,
    query_code_embedding: Vec<f32>,
    sparse_query_embedding: SparseVector,
    document_paths: Option<Vec<String>>,
    limit: Option<usize>,
    is_code: bool,
) -> Result<SearchResults, Box<dyn std::error::Error>> {
    let edge_shard = load_qdrant_edge()?;
    let top_k = limit.unwrap_or(RERANK_TOP_N);
    let stmt_filter = match document_paths {
        Some(d) => Some(Filter::new_must(Condition::Field(
            FieldCondition::new_match(
                JsonPath::from_str("document_path").map_err(|_| "invalid json path")?,
                Match::from(AnyVariants::Strings(d.iter().cloned().collect())),
            ),
        ))),
        None => None,
    };
    let weights = if is_code {
        vec![
            OrderedFloat(SPARSE_WEIGHT),
            OrderedFloat(DENSE_WEIGHT - 0.1),
            OrderedFloat(CODE_WEIGHT + 0.3),
        ]
    } else {
        vec![
            OrderedFloat(SPARSE_WEIGHT),
            OrderedFloat(DENSE_WEIGHT + 0.2),
            OrderedFloat(CODE_WEIGHT - 0.2),
        ]
    };
    let shard_query = QueryRequest {
        prefetches: vec![
            Prefetch {
                prefetches: vec![],
                query: Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
                    query: sparse_query_embedding.into(),
                    using: Some(SPARSE_VECTORS_NAME.to_string()),
                }))),
                limit: PREFETCH_TOP_N,
                params: None,
                filter: stmt_filter.clone(),
                score_threshold: None,
            },
            Prefetch {
                prefetches: vec![],
                query: Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
                    query: query_embedding.into(),
                    using: Some(VECTORS_NAME.to_string()),
                }))),
                limit: PREFETCH_TOP_N,
                params: None,
                filter: stmt_filter.clone(),
                score_threshold: None,
            },
            Prefetch {
                prefetches: vec![],
                query: Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
                    query: query_code_embedding.into(),
                    using: Some(CODE_VECTORS_NAME.to_string()),
                }))),
                limit: PREFETCH_TOP_N,
                params: None,
                filter: stmt_filter,
                score_threshold: None,
            },
        ],
        query: Some(ScoringQuery::Fusion(Fusion::Rrf {
            k: top_k,
            weights: Some(weights),
        })),
        filter: None,
        score_threshold: None,
        limit: top_k,
        offset: 0,
        params: None,
        with_vector: WithVector::Bool(false),
        with_payload: WithPayloadInterface::Bool(true),
    };
    let results = edge_shard.query(shard_query)?;
    let all_results = rerank(
        &edge_shard,
        original_query,
        results.clone(),
        TOP_K_FILES,
        TOP_CHUNKS_PER_FILE,
    )?;

    Ok(SearchResults {
        raw: results,
        processed: all_results,
    })
}
