use ignore::WalkBuilder;
use liteparse::{LiteParse, LiteParseConfig};
use model2vec_rs::model::StaticModel;
use qdrant_edge::{
    Condition, Distance, EdgeConfig, EdgeShard, EdgeVectorParams, FieldCondition, Filter, JsonPath,
    Match, PointId, PointInsertOperations, PointOperations, PointStruct, PointStructPersisted,
    UpdateOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

const FILES_INDEX: &str = ".microagents/files_index.json";
const SHARD_DIRECTORY: &str = ".microagents/vectors";
pub const VECTORS_NAME: &str = "dense_text";
const VECTORS_SIZE: usize = 256;
const BATCH_SIZE: usize = 1000;
pub const SUPPORTED_LIT_EXTENSIONS: &[&str] = &[
    ".pdf", ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tiff", ".webp", ".svg", ".doc", ".docx",
    ".docm", ".odt", ".rtf", ".ppt", ".pptx", ".pptm", ".odp", ".xls", ".xlsx", ".xlsm", ".ods",
    ".csv", ".tsv",
];
static EMBEDDING_MODEL: OnceLock<StaticModel> = OnceLock::new();
static PARSER: OnceLock<LiteParse> = OnceLock::new();
static EDGE_CONFIG: OnceLock<EdgeConfig> = OnceLock::new();

fn embedding_model() -> &'static StaticModel {
    EMBEDDING_MODEL.get_or_init(|| {
        StaticModel::from_pretrained("minishlab/potion-multilingual-128M", None, None, None)
            .expect("Should be able to get the embedding model")
    })
}

fn edge_config() -> &'static EdgeConfig {
    EDGE_CONFIG.get_or_init(|| EdgeConfig {
        on_disk_payload: true,
        vectors: HashMap::from([(
            VECTORS_NAME.to_string(),
            EdgeVectorParams {
                size: VECTORS_SIZE,
                distance: Distance::Cosine,
                on_disk: Some(true),
                quantization_config: None,
                multivector_config: None,
                datatype: None,
                hnsw_config: None,
            },
        )]),
        sparse_vectors: HashMap::new(),
        hnsw_config: Default::default(),
        quantization_config: None,
        optimizers: Default::default(),
    })
}

pub fn parser() -> &'static LiteParse {
    PARSER.get_or_init(|| LiteParse::new(LiteParseConfig::default()))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Document {
    path: String,
    fingerprint: String,
}

impl Document {
    fn has_been_modified(&self, other: &Document) -> bool {
        other.fingerprint != self.fingerprint
    }
}

struct Diff {
    modified: HashSet<String>,
    deleted: HashSet<String>,
    created: HashSet<String>,
}

impl Diff {
    fn is_no_diff(&self) -> bool {
        self.created.is_empty() && self.deleted.is_empty() && self.modified.is_empty()
    }

    fn to_reingest(&self) -> HashSet<String> {
        let mut combined = self.created.clone();
        combined.extend(self.modified.iter().cloned());
        combined
    }

    fn to_delete(&self) -> HashSet<String> {
        let mut combined = self.deleted.clone();
        combined.extend(self.modified.iter().cloned());
        combined
    }
}

impl Document {
    fn new(path: String, size: u64, mtime: u128) -> Self {
        Self {
            path,
            fingerprint: format!("{:?}-{:?}", size, mtime),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbeddingPayload {
    pub document_path: String,
    pub content: String,
}

struct EmbeddingWithPayload {
    embedding: Vec<f32>,
    payload: EmbeddingPayload,
}

/// Walk up from CWD looking for the files index. Returns `Ok(Some(root))` if
/// found, `Ok(None)` if no ancestor contains it, or `Err` on I/O errors.
fn resolve_root_path() -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let current_dir = std::env::current_dir()?;
    for ancestor in current_dir.ancestors() {
        if ancestor.join(FILES_INDEX).exists() {
            return Ok(Some(ancestor.to_path_buf()));
        }
    }
    Ok(None)
}

/// Resolve the project root, falling back to CWD if no index file exists yet.
/// Always returns an absolute path so downstream operations (file walking,
/// reading) are independent of the process CWD.
fn root_or_cwd() -> Result<PathBuf, Box<dyn std::error::Error>> {
    match resolve_root_path()? {
        Some(p) => Ok(p),
        None => Ok(std::env::current_dir()?),
    }
}

fn collect_files() -> Result<HashMap<String, Document>, Box<dyn std::error::Error>> {
    let mut paths: HashMap<String, Document> = HashMap::new();
    let root_path = root_or_cwd()?;
    let walker = WalkBuilder::new(&root_path)
        .hidden(false) // include dotfiles if needed
        .add_custom_ignore_filename(".microagentsignore")
        .build();
    for entry in walker {
        let entry = entry?;
        let path = entry.into_path();
        let meta = fs::metadata(&path)?;
        if path.is_file() {
            let repl = path.to_string_lossy().replace('\\', "/");
            paths.insert(
                repl.clone(),
                Document::new(
                    repl,
                    meta.size(),
                    meta.modified()?
                        .duration_since(SystemTime::UNIX_EPOCH)?
                        .as_millis(),
                ),
            );
        }
    }
    Ok(paths)
}

fn diff_files(files: HashMap<String, Document>) -> Result<Diff, Box<dyn std::error::Error>> {
    let new_content = serde_json::to_string(&files)?;
    let root_path = root_or_cwd()?;
    let p = root_path.join(FILES_INDEX);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    if p.exists() {
        let content = fs::read_to_string(&p)?;
        fs::write(&p, new_content)?;
        let fls: HashMap<String, Document> = serde_json::from_str(&content)?;
        let paths_now: HashSet<&str> = files.iter().map(|(k, _)| k.as_str()).collect();
        let paths_before: HashSet<&str> = fls.iter().map(|(k, _)| k.as_str()).collect();

        let created: HashSet<&str> = paths_now.difference(&paths_before).copied().collect();
        let deleted: HashSet<&str> = paths_before.difference(&paths_now).copied().collect();
        let mut modified: HashSet<&str> = HashSet::new();
        for (k, v) in fls.clone() {
            if !created.contains(k.as_str()) && !deleted.contains(k.as_str()) {
                let fl = files.get(&k).unwrap(); // we know that this is shared
                if fl.has_been_modified(&v) {
                    modified.insert(&fl.path);
                }
            }
        }

        return Ok(Diff {
            created: created.iter().map(|s| s.to_string()).collect(),
            deleted: deleted.iter().map(|s| s.to_string()).collect(),
            modified: modified.iter().map(|s| s.to_string()).collect(),
        });
    }
    fs::write(&p, new_content)?;

    Ok(Diff {
        created: files.keys().cloned().collect(),
        deleted: HashSet::new(),
        modified: HashSet::new(),
    })
}

fn chunk(content: String) -> Vec<String> {
    let chunks: Vec<&[u8]> = chunk::chunk(content.as_bytes()).size(4096).collect();

    let cs: Vec<String> = chunks
        .iter()
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect();
    cs
}

fn embed(sentences: &[String]) -> Vec<Vec<f32>> {
    let embeddings = embedding_model().encode(sentences);
    embeddings
}

pub fn embed_query(query: &str) -> Vec<f32> {
    let embeddings = embedding_model().encode(&[query.to_string()]);
    embeddings.into_iter().next().unwrap_or_default()
}

pub fn load_qdrant_edge() -> Result<EdgeShard, Box<dyn std::error::Error>> {
    let root_path = root_or_cwd()?;
    let p = root_path.join(SHARD_DIRECTORY);
    if p.exists() {
        let edge = EdgeShard::load(&p, Some(edge_config().clone()))?;
        return Ok(edge);
    }
    fs::create_dir_all(&p)?;
    let edge_shard = EdgeShard::new(&p, edge_config().clone())?;
    Ok(edge_shard)
}

/// Create a point struct for upserting.
fn make_point(id: uuid::Uuid, vector: Vec<f32>, payload: Value) -> PointStruct {
    PointStruct::new(
        PointId::Uuid(id),
        HashMap::from([(VECTORS_NAME.into(), vector)]),
        payload,
    )
}

fn upload_embeddings(eps: Vec<EmbeddingWithPayload>) -> Result<(), Box<dyn std::error::Error>> {
    if eps.is_empty() {
        return Ok(());
    }
    let edge = load_qdrant_edge()?;

    for chunk in eps.chunks(BATCH_SIZE) {
        let mut points: Vec<PointStructPersisted> = vec![];
        for embd in chunk {
            let payload_json = serde_json::to_value(&embd.payload)?;
            let point = make_point(uuid::Uuid::new_v4(), embd.embedding.clone(), payload_json);
            points.push(point.into());
        }
        let operation = UpdateOperation::PointOperation(PointOperations::UpsertPoints(
            PointInsertOperations::PointsList(points),
        ));
        edge.update(operation)?;
    }

    Ok(())
}

async fn ingest_files(to_ingest: HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
    if to_ingest.is_empty() {
        return Ok(());
    }
    let root_path = root_or_cwd()?;
    let mut failed = HashSet::new();
    let mut embeddings_with_payloads: Vec<EmbeddingWithPayload> = Vec::new();
    for fl in to_ingest {
        let abs_path = root_path.join(&fl);
        let ext = Path::new(&fl)
            .extension()
            .unwrap_or_default()
            .to_str()
            .map(|s| format!(".{}", s.to_lowercase()))
            .unwrap_or_default();
        let content = if SUPPORTED_LIT_EXTENSIONS.contains(&ext.as_str()) {
            let abs_str = abs_path.to_string_lossy().to_string();
            match parser().parse(&abs_str).await {
                Err(_) => {
                    failed.insert(fl);
                    continue;
                }
                Ok(p) => p.text,
            }
        } else {
            match fs::read_to_string(&abs_path) {
                Err(_) => {
                    failed.insert(fl);
                    continue;
                }
                Ok(p) => p,
            }
        };

        let chunks = chunk(content);
        if chunks.is_empty() {
            continue;
        }
        let vectors = embed(&chunks);
        for (chunk_text, embedding) in chunks.into_iter().zip(vectors.into_iter()) {
            embeddings_with_payloads.push(EmbeddingWithPayload {
                embedding,
                payload: EmbeddingPayload {
                    document_path: fl.clone(),
                    content: chunk_text,
                },
            });
        }
    }

    upload_embeddings(embeddings_with_payloads)?;

    if !failed.is_empty() {
        eprintln!(
            "Warning: failed to ingest {} file(s): {:?}",
            failed.len(),
            failed
        );
    }

    Ok(())
}

fn delete_files(to_delete: HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
    if to_delete.is_empty() {
        return Ok(());
    }
    let edge = load_qdrant_edge()?;
    for path in to_delete {
        let condition = Condition::Field(FieldCondition::new_match(
            "document_path"
                .parse::<JsonPath>()
                .map_err(|_| "invalid json path")?,
            Match::from(path),
        ));
        let filter = Filter::new_must(condition);
        let operation =
            UpdateOperation::PointOperation(PointOperations::DeletePointsByFilter(filter));
        edge.update(operation)?;
    }
    Ok(())
}

pub async fn initialize_environment() -> Result<(), Box<dyn std::error::Error>> {
    let root_path = root_or_cwd()?;
    if !root_path.join(".microagents").exists() {
        fs::create_dir_all(root_path.join(".microagents"))?;
    }

    let _ = embedding_model();
    let _ = edge_config();
    let _ = parser();

    let files = collect_files()?;
    println!("Collected all the files in the current directory...");
    let diff = diff_files(files)?;

    println!(
        "Computed diff for files: re-ingesting {:?} file(s), deleting {:?}",
        diff.to_reingest().len(),
        diff.deleted.len()
    );

    if diff.is_no_diff() {
        println!("No changes to apply!");
        return Ok(());
    }

    println!("Applying changes to detected diff files...");

    delete_files(diff.to_delete())?;
    ingest_files(diff.to_reingest()).await?;

    Ok(())
}
