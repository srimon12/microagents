use qdrant_edge::{
    AnyVariants, Condition, FieldCondition, Filter, Fusion, JsonPath, Match, NamedQuery, Payload,
    Prefetch, QueryEnum, QueryRequest, ScoringQuery, SparseVector, WithPayloadInterface,
    WithVector, external::ordered_float::OrderedFloat,
};
use std::str::FromStr;

use crate::init_env::{EmbeddingPayload, SPARSE_VECTORS_NAME, VECTORS_NAME, load_qdrant_edge};

#[derive(Debug, Clone)]
pub struct ResultWithScore {
    pub content: String,
    pub document_path: String,
    pub score: f32,
}

/// Convert Qdrant Payload back to DocMeta
fn payload_to_struct(payload: &Payload) -> Result<EmbeddingPayload, Box<dyn std::error::Error>> {
    let ep: EmbeddingPayload = serde_json::from_value(serde_json::to_value(payload)?)?;
    Ok(ep)
}

pub fn search(
    query_embedding: Vec<f32>,
    sparse_query_embedding: SparseVector,
    document_paths: Option<Vec<String>>,
    limit: Option<usize>,
    score_threshold: Option<f32>,
) -> Result<Vec<ResultWithScore>, Box<dyn std::error::Error>> {
    let edge_shard = load_qdrant_edge()?;
    let mut all_results: Vec<ResultWithScore> = vec![];
    let threshold: Option<OrderedFloat<f32>> = score_threshold.map(OrderedFloat);
    let top_k = limit.unwrap_or(10);
    let stmt_filter = match document_paths {
        Some(d) => Some(Filter::new_must(Condition::Field(
            FieldCondition::new_match(
                JsonPath::from_str("document_path").map_err(|_| "invalid json path")?,
                Match::from(AnyVariants::Strings(d.iter().cloned().collect())),
            ),
        ))),
        None => None,
    };
    let shard_query = QueryRequest {
        prefetches: vec![
            Prefetch {
                prefetches: vec![],
                query: Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
                    query: sparse_query_embedding.into(),
                    using: Some(SPARSE_VECTORS_NAME.to_string()),
                }))),
                limit: top_k,
                params: None,
                filter: None,
                score_threshold: threshold,
            },
            Prefetch {
                prefetches: vec![],
                query: Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
                    query: query_embedding.into(),
                    using: Some(VECTORS_NAME.to_string()),
                }))),
                limit: top_k,
                params: None,
                filter: None,
                score_threshold: None,
            },
        ],
        query: Some(ScoringQuery::Fusion(Fusion::Rrf {
            k: 2,
            weights: Some(vec![OrderedFloat(0.75), OrderedFloat(0.25)]),
        })),
        filter: stmt_filter,
        score_threshold: None,
        limit: top_k,
        offset: 0,
        params: None,
        with_vector: WithVector::Bool(false),
        with_payload: WithPayloadInterface::Bool(true),
    };
    let results = edge_shard.query(shard_query)?;
    for r in results {
        let payload = match r.payload {
            Some(p) => p,
            None => return Err("Found a None payload when searching".into()),
        };
        let embd_payload = payload_to_struct(&payload)?;
        let scored_result = ResultWithScore {
            content: embd_payload.content,
            document_path: embd_payload.document_path,
            score: r.score,
        };
        all_results.push(scored_result);
    }

    Ok(all_results)
}
