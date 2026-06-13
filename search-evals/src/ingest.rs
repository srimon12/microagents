use microagents_cli::{
    init_env::{edge_config, initialize_environment},
    processing::{CODE_EMBEDDING_MODEL_NAME, TEXT_EMBEDDING_MODEL_NAME, bm25_config, code_chunker},
    search::{
        CODE_WEIGHT, DENSE_WEIGHT, PREFETCH_TOP_N, RERANK_TOP_N, SPARSE_WEIGHT,
        TOP_CHUNKS_PER_FILE, TOP_K_FILES,
    },
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
    let code_chunker = code_chunker();
    let ast_chunking_options = json!({
        "max_nws_size": &code_chunker.options.max_nws_size,
        "overlap_nodes": &code_chunker.options.overlap_nodes
    });
    let bm25_embedder_options = bm25_config();
    let ingestion_config = json!({
       "qdrant_edge_config": &edge_config,
       "text_embeddng_model_name": TEXT_EMBEDDING_MODEL_NAME,
       "code_embeddng_model_name": CODE_EMBEDDING_MODEL_NAME,
       "ast_chunking_options": ast_chunking_options,
       "bm25_embedder_options": bm25_embedder_options,
       "rerank_top_n": RERANK_TOP_N,
       "dense_weight": DENSE_WEIGHT - 0.2,
       "sparse_weight": SPARSE_WEIGHT,
       "code_weight": CODE_WEIGHT + 0.2,
       "prefetch_top_n": PREFETCH_TOP_N,
       "top_k_files": TOP_K_FILES,
       "top_chunks_per_file": TOP_CHUNKS_PER_FILE,
    });
    let content = serde_json::to_string_pretty(&ingestion_config)?;
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
