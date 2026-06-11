use microagents_cli::{
    init_env::{edge_config, initialize_environment},
    processing::{EMBEDDING_MODEL_NAME, bm25_config, code_chunker},
    search::{DENSE_WEIGHT, RERANK_TOP_N, SPARSE_WEIGHT},
};
use serde_json::json;
use std::{fs, path::Path};

const MICROAGENTS_DIR: &str = ".microagents/";

fn cleanup_local_microagents_dir() -> anyhow::Result<()> {
    fs::remove_dir_all(MICROAGENTS_DIR)?;
    Ok(())
}

fn write_config(experiment_name: &str) -> anyhow::Result<()> {
    fs::create_dir_all("evals/configs")
        .map_err(|e| anyhow::anyhow!(format!("an error occurred: {}", e)))?;
    let file = Path::new("evals/configs").join(format!("experiment-{}.json", experiment_name));
    let edge_config = edge_config();
    let embedding_model_name = EMBEDDING_MODEL_NAME;
    let code_chunker = code_chunker();
    let ast_chunking_options = json!({
        "max_nws_size": &code_chunker.options.max_nws_size,
        "overlap_nodes": &code_chunker.options.overlap_nodes
    });
    let bm25_embedder_options = bm25_config();
    let ingestion_config = json!({
       "qdrant_edge_config": &edge_config,
       "embedding_model_name": embedding_model_name,
       "ast_chunking_options": ast_chunking_options,
       "bm25_embedder_options": bm25_embedder_options,
       "rerank_top_n": RERANK_TOP_N,
       "dense_weight": DENSE_WEIGHT,
       "sparse_weight": SPARSE_WEIGHT,
    });
    let content = serde_json::to_string(&ingestion_config)?;
    fs::write(file, content)?;
    Ok(())
}

pub async fn ingest(experiment_name: &str) -> anyhow::Result<()> {
    if Path::new(MICROAGENTS_DIR).exists() {
        cleanup_local_microagents_dir()?;
    }
    write_config(experiment_name)?;
    initialize_environment(false)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
