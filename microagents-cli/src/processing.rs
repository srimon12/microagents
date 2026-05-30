use std::sync::OnceLock;

use astchunk::{
    chunker::{CastChunker, CastChunkerOptions, Chunker},
    lang::Language,
    types::{Document, DocumentId, Origin},
};
use qdrant_edge::SparseVector;
use qdrant_edge::bm25_embed::{EdgeBm25, EdgeBm25Config};

pub struct Chunk {
    content: String,
    line_start: Option<u32>,
    line_end: Option<u32>,
    embedding: Option<Vec<f32>>,
    sparse_embedding: Option<SparseVector>,
}

impl Chunk {
    pub fn new(content: String, line_start: Option<u32>, line_end: Option<u32>) -> Self {
        Self {
            content,
            line_end,
            line_start,
            embedding: None,
            sparse_embedding: None,
        }
    }

    pub fn embedding(mut self, embd: Vec<f32>) -> Self {
        self.embedding = Some(embd);
        self
    }

    pub fn sparse_embedding(mut self, sparse_embd: SparseVector) -> Self {
        self.sparse_embedding = Some(sparse_embd);
        self
    }
}

const CODE_CHUNKER: OnceLock<CastChunker> = OnceLock::new();

fn code_chunker() -> &'static CastChunker {
    CODE_CHUNKER.get_or_init(|| CastChunker {
        options: CastChunkerOptions::default(),
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

fn reconstruct_content(lines: Vec<&str>, line_start: usize, line_end: usize) -> String {
    lines[line_start..=line_end].join("\n")
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
                lines,
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
