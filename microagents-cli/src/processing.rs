use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use astchunk::{
    chunker::{CastChunker, CastChunkerOptions, Chunker},
    lang::Language,
    types::{Document, DocumentId, Origin},
};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use qdrant_edge::bm25_embed::{EdgeBm25, EdgeBm25Config};
use qdrant_edge::{SparseVector, external::ordered_float::NotNan};

#[allow(dead_code)]
pub const CODE_EMBEDDING_MODEL_NAME: &str = "jinaai/jina-embeddings-v2-base-code";
#[allow(dead_code)]
pub const TEXT_EMBEDDING_MODEL_NAME: &str = "jinaai/jina-embeddings-v2-base-en";

#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub embedding: Option<Vec<f32>>,
    pub code_embedding: Option<Vec<f32>>,
    pub sparse_embedding: Option<SparseVector>,
}

impl Chunk {
    pub fn new(content: String, line_start: Option<u32>, line_end: Option<u32>) -> Self {
        Self {
            content,
            line_end,
            line_start,
            code_embedding: None,
            embedding: None,
            sparse_embedding: None,
        }
    }
}

static CODE_CHUNKER: OnceLock<CastChunker> = OnceLock::new();
static BM25_EMBEDDER: OnceLock<EdgeBm25> = OnceLock::new();
static BM25_CONFIG: OnceLock<EdgeBm25Config> = OnceLock::new();
static TEXT_EMBEDDING_MODEL: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();
static CODE_EMBEDDING_MODEL: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();
pub static FASTEMBED_CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn fastembed_cache_dir() -> &'static PathBuf {
    FASTEMBED_CACHE_DIR.get_or_init(|| {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(".microagents")
            .join("fastembed-cache")
    })
}

pub fn bm25_config() -> &'static EdgeBm25Config {
    BM25_CONFIG.get_or_init(|| EdgeBm25Config {
        avg_len: unsafe { NotNan::new_unchecked(1500_f64) },
        ..Default::default()
    })
}

pub fn code_chunker() -> &'static CastChunker {
    CODE_CHUNKER.get_or_init(|| {
        let mut c = CastChunker {
            options: CastChunkerOptions::default(),
        };
        c.options.overlap_nodes = 30;
        c
    })
}

pub fn bm25_embedder() -> &'static EdgeBm25 {
    BM25_EMBEDDER.get_or_init(|| {
        EdgeBm25::new(bm25_config().to_owned()).expect("Should be able to get BM25 embedder")
    })
}

fn text_embedding_model() -> &'static Mutex<TextEmbedding> {
    TEXT_EMBEDDING_MODEL.get_or_init(|| {
        Mutex::new(
            TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::JinaEmbeddingsV2BaseEN)
                    .with_show_download_progress(true)
                    .with_intra_threads(2)
                    .with_cache_dir(fastembed_cache_dir().to_owned()),
            )
            .expect("Should be able to load text embedding model"),
        )
    })
}

fn code_embedding_model() -> &'static Mutex<TextEmbedding> {
    CODE_EMBEDDING_MODEL.get_or_init(|| {
        Mutex::new(
            TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::JinaEmbeddingsV2BaseCode)
                    .with_show_download_progress(true)
                    .with_intra_threads(2)
                    .with_cache_dir(fastembed_cache_dir().to_owned()),
            )
            .expect("Should be able to load text embedding model"),
        )
    })
}

fn infer_language_from_extension(ext: &str) -> Option<Language> {
    match ext {
        ".cs" => Some(Language::CSharp),
        ".cpp" => Some(Language::Cpp),
        ".java" => Some(Language::Java),
        ".js" => Some(Language::TypeScript),
        ".ts" => Some(Language::TypeScript),
        ".tsx" => Some(Language::TypeScript),
        ".jsx" => Some(Language::TypeScript),
        ".rs" => Some(Language::Rust),
        ".py" => Some(Language::Python),
        _ => None,
    }
}

fn reconstruct_content(lines: &[&str], line_start: usize, line_end: usize) -> String {
    lines[line_start..line_end - 1].join("\n")
}

fn chunk_code(lang: Language, source: &str) -> Result<Vec<Chunk>, Box<dyn std::error::Error>> {
    let document = Document {
        document_id: DocumentId(0),
        source: source.into(),
        language: lang,
        origin: Origin::default(),
    };
    let lines: Vec<&str> = source.lines().collect();
    let ast_chunks = code_chunker().chunk(&document)?;
    let mut chunks: Vec<Chunk> = vec![];
    for c in ast_chunks {
        let ch = Chunk::new(
            reconstruct_content(
                &lines,
                c.line_index_range.start as usize,
                c.line_index_range.end as usize,
            ),
            Some(c.line_index_range.start),
            Some(c.line_index_range.end),
        );
        chunks.push(ch);
    }
    Ok(chunks)
}

fn chunk_text(content: &str) -> Vec<Chunk> {
    let chunks: Vec<&[u8]> = chunk::chunk(content.as_bytes()).size(1024).collect();

    let cs: Vec<Chunk> = chunks
        .iter()
        .map(|c| Chunk::new(String::from_utf8_lossy(c).to_string(), None, None))
        .collect();
    cs
}

pub fn chunk(extension: &str, content: String) -> Result<Vec<Chunk>, Box<dyn std::error::Error>> {
    let chunks: Vec<Chunk> = if let Some(lang) = infer_language_from_extension(extension) {
        chunk_code(lang, &content)?
    } else {
        chunk_text(&content)
    };
    Ok(chunks)
}

pub fn embed(mut chunks: Vec<Chunk>) -> Vec<Chunk> {
    let bm25 = bm25_embedder();
    let embedder_mu = text_embedding_model();
    let code_embedder_mu = code_embedding_model();
    let mut embedder = embedder_mu
        .lock()
        .expect("Mutex on text embedding model has been poisoned");
    let mut code_embedder = code_embedder_mu
        .lock()
        .expect("Mutex on code embedding model has been poisoned");
    for c in &mut *chunks {
        let sparse_embd = bm25.embed_document(&c.content);
        let dense_embd = embedder
            .embed(vec![&format!("passage: {}", c.content)], None)
            .expect("Should be able to create text embedding");
        let code_embd = code_embedder
            .embed(vec![&format!("passage: {}", c.content)], None)
            .expect("Should be able to create code embedding");
        c.embedding = Some(<Vec<f32> as Clone>::clone(&dense_embd[0]));
        c.code_embedding = Some(<Vec<f32> as Clone>::clone(&code_embd[0]));
        c.sparse_embedding = Some(sparse_embd);
    }
    chunks
}

pub fn embed_query(query: &str) -> (Vec<f32>, Vec<f32>, SparseVector) {
    let bm25 = bm25_embedder();
    let embedder_mu = text_embedding_model();
    let code_embedder_mu = code_embedding_model();
    let mut embedder = embedder_mu
        .lock()
        .expect("Mutex on text embedding model has been poisoned");
    let mut code_embedder = code_embedder_mu
        .lock()
        .expect("Mutext on code embedding model has been poisoned");
    let dense_embd = embedder
        .embed(vec![&format!("query: {}", query)], None)
        .expect("Should be able to embed query");
    let code_embd = code_embedder
        .embed(vec![&format!("query: {}", query)], None)
        .expect("Should be able to embed query");
    let sparse_embd = bm25.embed_query(query);
    (
        <Vec<f32> as Clone>::clone(&dense_embd[0]),
        <Vec<f32> as Clone>::clone(&code_embd[0]),
        sparse_embd,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_language_from_extension() {
        let test_cases: Vec<(&str, Option<Language>)> = vec![
            (".ts", Some(Language::TypeScript)),
            (".rs", Some(Language::Rust)),
            (".cpp", Some(Language::Cpp)),
            (".cs", Some(Language::CSharp)),
            (".java", Some(Language::Java)),
            (".js", Some(Language::TypeScript)),
            (".tsx", Some(Language::TypeScript)),
            (".jsx", Some(Language::TypeScript)),
            (".py", Some(Language::Python)),
            (".go", None),
            (".zig", None),
        ];
        for tc in test_cases {
            let inferred = infer_language_from_extension(tc.0);
            assert_eq!(inferred, tc.1);
        }
    }

    #[test]
    fn test_code_chunking_produces_single_chunk() {
        let rust_function = r#"fn hello_world() {
    println!("Hello, world!");
}"#;
        let chunks = chunk(".rs", rust_function.to_string()).unwrap();
        assert_eq!(
            chunks.len(),
            1,
            "Expected a single chunk for a small function"
        );
        assert!(chunks[0].content.contains("hello_world"));
    }

    #[test]
    fn test_text_chunking_small_paragraph() {
        let paragraph = "This is a small paragraph with less than 1024 characters.";
        assert!(paragraph.len() < 1024);
        let chunks = chunk(".txt", paragraph.to_string()).unwrap();
        assert_eq!(
            chunks.len(),
            1,
            "Expected a single chunk for text under 1024 chars"
        );
        assert_eq!(chunks[0].content, paragraph);
    }

    #[test]
    fn test_chunking_routes_code_vs_text() {
        let code = "fn main() {}";
        let text = "Just some plain text content.";

        let code_chunks = chunk(".rs", code.to_string()).unwrap();
        let text_chunks = chunk(".unknown", text.to_string()).unwrap();

        assert_eq!(code_chunks.len(), 1);
        assert_eq!(text_chunks.len(), 1);

        assert!(
            code_chunks[0].line_start.is_some(),
            "Code chunks should have line numbers"
        );
        assert!(
            code_chunks[0].line_end.is_some(),
            "Code chunks should have line numbers"
        );
        assert!(
            text_chunks[0].line_start.is_none(),
            "Text chunks should not have line numbers"
        );
        assert!(
            text_chunks[0].line_end.is_none(),
            "Text chunks should not have line numbers"
        );
    }
}
