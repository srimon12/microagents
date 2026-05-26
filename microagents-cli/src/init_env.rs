use ignore::WalkBuilder;
use std::collections::HashMap;
use std::fs;
use std::time::SystemTime;

pub fn collect_files() -> Result<HashMap<String, (u64, u128)>, Box<dyn std::error::Error>> {
    let mut paths: HashMap<String, (u64, u128)> = HashMap::new();
    let walker = WalkBuilder::new(".")
        .hidden(false) // include dotfiles if needed
        .build();
    for entry in walker {
        let entry = entry?;
        let path = entry.into_path();
        let meta = fs::metadata(&path)?;
        if path.is_file() {
            let new_path = path.to_string_lossy().replace("\\", "/");
            let repl = new_path.trim_start_matches("./").to_string();
            paths.insert(
                repl,
                (
                    meta.len(),
                    meta.modified()?
                        .duration_since(SystemTime::UNIX_EPOCH)?
                        .as_millis(),
                ),
            );
        }
    }
    Ok(paths)
}
