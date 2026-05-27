use qdrant_edge::{
    AnyVariants, Condition, FieldCondition, Filter, JsonPath, Match, NamedQuery, Payload,
    QueryEnum, QueryRequest, ScoringQuery, WithPayloadInterface, WithVector,
    external::ordered_float::OrderedFloat,
};
use serde_json::Value;
use std::str::FromStr;

use crate::init_env::{EmbeddingPayload, VECTORS_NAME, load_qdrant_edge};

pub struct ResultWithScore {
    pub content: String,
    pub document_path: String,
    pub score: f64,
}

/// Convert Qdrant Payload back to DocMeta
fn payload_to_struct(payload: &Payload) -> Result<EmbeddingPayload, Box<dyn std::error::Error>> {
    let json_map: serde_json::Map<String, Value> = payload
        .0
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let json_value = Value::Object(json_map);
    let ep: EmbeddingPayload = serde_json::from_value(json_value)?;
    Ok(ep)
}

pub fn search(
    query_embedding: Vec<f32>,
    document_paths: Option<Vec<String>>,
    limit: Option<u32>,
    score_threshold: Option<f64>,
) -> Result<Vec<ResultWithScore>, Box<dyn std::error::Error>> {
    let edge_shard = load_qdrant_edge()?;
    let mut all_results: Vec<ResultWithScore> = vec![];
    let threshold: Option<OrderedFloat<f32>> = score_threshold.map(|t| OrderedFloat(t as f32));
    let top_k = match limit {
        Some(l) => l as usize,
        None => 10,
    };
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
        prefetches: vec![],
        query: Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery {
            query: query_embedding.into(),
            using: Some(VECTORS_NAME.to_string()),
        }))),
        filter: stmt_filter,
        score_threshold: threshold,
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
            score: r.score as f64,
        };
        all_results.push(scored_result);
    }

    Ok(all_results)
}
