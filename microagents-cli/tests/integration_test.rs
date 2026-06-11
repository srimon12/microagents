use std::path::Path;

use microagents_cli::{
    init_env::{initialize_environment, load_qdrant_edge},
    processing::{chunk, embed, embed_query},
    search::search,
};
use qdrant_edge::CountRequest;
use std::fs;

fn create_test_files(dir: &Path) {
    fs::write(dir.join("test.py"), "print('Hello World')")
        .expect("Should be able to write the file");
    fs::write(dir.join("test.ts"), "console.log('Hello World')")
        .expect("Should be able to write file");
    fs::write(dir.join("test.pdf"), br#"%PDF-1.4
    1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
    2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
    3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>endobj
    4 0 obj<</Length 44>>
    stream
    BT /F1 12 Tf 100 700 Td (Hello World) Tj ET
    endstream
    endobj
    5 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj
    xref
    0 6
    0000000000 65535 f
    0000000009 00000 n
    0000000058 00000 n
    0000000115 00000 n
    0000000274 00000 n
    0000000368 00000 n
    trailer<</Size 6/Root 1 0 R>>
    startxref
    441
    %%EOF"#).expect("Should be able to write file");
    fs::write(".microagentsignore", ".microagents/").expect("Should be able to write file");
}

fn modify_test_file(dir: &Path) {
    fs::write(dir.join("test.py"), "print('Hello Moon!')")
        .expect("Should be able to overwrite the file");
}

fn delete_test_file(dir: &Path) {
    fs::remove_file(dir.join("test.ts")).expect("Should be able to remove the file");
}

fn add_test_file(dir: &Path) {
    fs::write(dir.join("test.js"), "console.log('Hello world from JS')")
        .expect("Should be able to add a file");
}

#[tokio::test]
#[serial_test::serial]
async fn test_initialize_environment() {
    match std::env::var("RUN_CLI_TESTS") {
        Ok(v) => {
            if v != "true" {
                return;
            }
        }
        Err(_) => return,
    }
    let cur_dir = std::env::current_dir().expect("Should be able to find a current directory");
    let tmp = tempfile::tempdir().expect("Should be able to create a temporary directory");
    std::env::set_current_dir(tmp.path())
        .expect("Should be able to change the current working directory");
    create_test_files(tmp.path());
    let (created, modified, deleted) = initialize_environment(false)
        .await
        .expect("Environment initialization should not fail");
    assert_eq!(created, 4);
    assert_eq!(modified, 0);
    assert_eq!(deleted, 0);
    modify_test_file(tmp.path());
    let (created_1, modified_1, deleted_1) = initialize_environment(false)
        .await
        .expect("Environment initialization should not fail");
    assert_eq!(created_1, 0);
    assert_eq!(modified_1, 1);
    assert_eq!(deleted_1, 0);
    delete_test_file(tmp.path());
    let (created_2, modified_2, deleted_2) = initialize_environment(false)
        .await
        .expect("Environment initialization should not fail");
    assert_eq!(created_2, 0);
    assert_eq!(modified_2, 0);
    assert_eq!(deleted_2, 1);
    add_test_file(tmp.path());
    let (created_3, modified_3, deleted_3) = initialize_environment(false)
        .await
        .expect("Environment initialization should not fail");
    assert_eq!(created_3, 1);
    assert_eq!(modified_3, 0);
    assert_eq!(deleted_3, 0);
    let qd = load_qdrant_edge().expect("Should be able to load the vector store");
    let count = qd
        .count(CountRequest {
            filter: None,
            exact: true,
        })
        .expect("Should be able to get the vector count for the vector store");
    assert!(count >= 4); // 4 files, at least 1 point per file
    std::env::set_current_dir(cur_dir)
        .expect("Should be able to change the current working directory");
}

#[tokio::test]
#[serial_test::serial]
async fn test_search_vanilla() {
    match std::env::var("RUN_CLI_TESTS") {
        Ok(v) => {
            if v != "true" {
                return;
            }
        }
        Err(_) => return,
    }
    let cur_dir = std::env::current_dir().expect("Should be able to find a current directory");
    let tmp = tempfile::tempdir().expect("Should be able to create a temporary directory");
    std::env::set_current_dir(tmp.path())
        .expect("Should be able to change the current working directory");
    create_test_files(tmp.path());
    initialize_environment(false)
        .await
        .expect("Environment initialization should not fail");
    let (dense, sparse) = embed_query("Hello World");
    let results = search(dense, sparse, None, None, None).expect("Search should not fail");
    assert!(results.processed.len() >= 3);
    assert!(
        results
            .processed
            .iter()
            .any(|r| r.content.contains("Hello World"))
    );
    std::env::set_current_dir(cur_dir)
        .expect("Should be able to change the current working directory");
}

#[tokio::test]
#[serial_test::serial]
async fn test_search_filters() {
    match std::env::var("RUN_CLI_TESTS") {
        Ok(v) => {
            if v != "true" {
                return;
            }
        }
        Err(_) => return,
    }
    let cur_dir = std::env::current_dir().expect("Should be able to find a current directory");
    let tmp = tempfile::tempdir().expect("Should be able to create a temporary directory");
    std::env::set_current_dir(tmp.path())
        .expect("Should be able to change the current working directory");
    create_test_files(tmp.path());
    initialize_environment(false)
        .await
        .expect("Environment initialization should not fail");
    let (dense, sparse) = embed_query("console.log('Hello World')");
    let ts_path = tmp
        .path()
        .join("test.ts")
        .canonicalize()
        .expect("Should canonicalize test.ts");
    let results = search(
        dense,
        sparse,
        Some(vec![ts_path.to_str().unwrap().to_string()]),
        None,
        None,
    )
    .expect("Search should not fail");
    assert_eq!(results.processed.len(), 1);
    assert!(
        results
            .processed
            .iter()
            .all(|r| r.document_path.contains("test.ts"))
    );
    std::env::set_current_dir(cur_dir)
        .expect("Should be able to change the current working directory");
}

#[tokio::test]
#[serial_test::serial]
async fn test_search_filters_no_exist() {
    match std::env::var("RUN_CLI_TESTS") {
        Ok(v) => {
            if v != "true" {
                return;
            }
        }
        Err(_) => return,
    }
    let cur_dir = std::env::current_dir().expect("Should be able to find a current directory");
    let tmp = tempfile::tempdir().expect("Should be able to create a temporary directory");
    std::env::set_current_dir(tmp.path())
        .expect("Should be able to change the current working directory");
    create_test_files(tmp.path());
    initialize_environment(false)
        .await
        .expect("Environment initialization should not fail");
    let (dense, sparse) = embed_query("console.log('Hello World')");
    let results = search(dense, sparse, Some(vec!["test.js".to_string()]), None, None)
        .expect("Search should not fail");
    assert_eq!(results.processed.len(), 0);
    std::env::set_current_dir(cur_dir)
        .expect("Should be able to change the current working directory");
}

#[tokio::test]
#[serial_test::serial]
async fn test_search_limit() {
    match std::env::var("RUN_CLI_TESTS") {
        Ok(v) => {
            if v != "true" {
                return;
            }
        }
        Err(_) => return,
    }
    let cur_dir = std::env::current_dir().expect("Should be able to find a current directory");
    let tmp = tempfile::tempdir().expect("Should be able to create a temporary directory");
    std::env::set_current_dir(tmp.path())
        .expect("Should be able to change the current working directory");
    create_test_files(tmp.path());
    initialize_environment(false)
        .await
        .expect("Environment initialization should not fail");
    let (dense, sparse) = embed_query("Hello World");
    let results = search(dense, sparse, None, Some(2), None).expect("Search should not fail");
    assert_eq!(results.processed.len(), 2);
    std::env::set_current_dir(cur_dir)
        .expect("Should be able to change the current working directory");
}

#[test]
fn test_chunking_and_embedding() {
    match std::env::var("RUN_CLI_TESTS") {
        Ok(v) => {
            if v != "true" {
                return;
            }
        }
        Err(_) => return,
    }
    let code = r#"
def hello_world() -> None:
    print('Hello world!')

if __name__ == '__main__':
    hello_world()
"#;
    let text = "This is a small text paragraph that should be chunked as one";
    let mut chunks_code =
        chunk(".py", code.to_string()).expect("Should be able to chunk the given text");
    let mut chunks_text =
        chunk(".txt", text.to_string()).expect("Should be able to chunk the given text");
    chunks_code.append(&mut chunks_text);
    chunks_code = embed(chunks_code);
    assert!(chunks_code.len() >= 2);
    for c in chunks_code {
        assert!(c.embedding.is_some_and(|e| e.len() == 256));
        assert!(c.sparse_embedding.is_some());
        if c.line_start.is_some() && c.line_end.is_some() {
            assert!(c.content.contains("hello_world"));
        }
    }
}
