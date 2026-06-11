use microagents_cli::{processing::embed_query, search::search};
use serde_json::{Value, json};
use std::fs;

use crate::load_questions::Question;

pub fn search_queries(experiment_name: &str, questions: Vec<Question>) -> anyhow::Result<()> {
    fs::create_dir_all(format!("evals/experiment-{}/", experiment_name))?;
    let mut i = 0;
    for question in questions {
        i += 1;
        let (dense, sparse) = embed_query(&question.query);
        let results =
            search(dense, sparse, None, None, None).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let mut raw_results: Vec<Value> = Vec::with_capacity(results.raw.len());
        for r in results.raw {
            let v = json!({
                "id": &r.id,
                "score": &r.score,
                "payload": &r.payload,
                "order_value": &r.order_value,
            });
            raw_results.push(v);
        }
        let json_results = json!({
            "processed": &results.processed,
            "raw": &raw_results,
        });
        let file = format!("evals/experiment-{}/{:?}.json", experiment_name, i);
        fs::write(file, serde_json::to_string(&json_results)?)?;
    }

    Ok(())
}
