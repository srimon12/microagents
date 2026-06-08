use microagents_cli::init_env::root_or_cwd;
use microagents_core::types::{AgentError, ToolExecutionContext, ToolFunction};
use microagents_events::types::ToolResult;
use regex::Regex;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;

use tokio::process::Command;

use crate::init_env::{PARSER_MUTEX, SUPPORTED_LIT_EXTENSIONS, parser};
use crate::processing::embed_query;
use crate::search::search as vector_search;

static DANGEROUS: OnceLock<Regex> = OnceLock::new();

fn dangerous_regex() -> &'static Regex {
    DANGEROUS.get_or_init(|| {
        Regex::new(
            r#"(?i)(?:
            \brm\s+-[rf]{1,2}
            |\bmkfs
            |\bdd\s+if=
            |\bshred\s+
            |\bwipefs\s+
            |\bchmod\s+[0-7]*7{2,}
            |\bchown\s+-R
            |\bsudo\s+
            |\bsu\s+
            |\bwget\s+\S+\s*\|\s*sh
            |\bcurl\s+\S+\s*\|\s*sh
            |\bnc\s+-e
            |\bbash\s+-i
            |\bpython[23]?\s+-c
            |\bperl\s+-e
            |\bshutdown\b
            |\breboot\b
            |\bhalt\b
            |\binit\s+0
            |\bkill\s+-9\s+-1
            |/etc/(?:passwd|shadow|sudoers)
            |(?:>|>>)\s*/dev/sd[a-z]
            |:\(\)\{.+\}
            |\brd\s+/s
            |\brmdir\s+/s
            |\bdel\s+/[fqs]
            |\bformat\s+[a-z]:
            |\bdiskpart\b
            |\bshutdown\s+/[rfs]
            |\btaskkill\s+/f
            |\bnet\s+(?:stop|user|localgroup)
            |\bsc\s+(?:delete|stop|create)
            |\breg\s+(?:delete|add|import|export)
            |\bwhoami\s+/priv
            |\bicacls\b
            |\battrib\s+[+-][rsah]
            |\bschtasks\s+/create
            |\bbcdedit\b
            |\bbootrec\b
            |[a-z]:\\(?:windows\\system32\\config\\(?:sam|system|security)|windows\\repair\\sam)
            |\bInvoke-Expression\b
            |\bIEX\b
            |\bInvoke-WebRequest\b.*\|\s*I(?:EX|nvoke-Expression)
            |\bDownloadString\b.*\|\s*I(?:EX|nvoke-Expression)
            |\bStart-Process\b
            |\bInvoke-Command\b
            |\bEncodedCommand\b
            |\bFromBase64String\b
            |\b-enc\b
            |\b-ex\s+bypass\b
            |\bExecutionPolicy\s+(?:bypass|unrestricted)
            |\bGet-Credential\b
            |\bDump-(?:Lsass|SAM)\b
            |\bAdd-MpPreference\b
            |\bSet-MpPreference\b
            |\bDisable-WindowsOptionalFeature\b
        )"#
            .replace('\n', "")
            .trim(),
        )
        .unwrap()
    })
}

fn is_dangerous(cmd: &str) -> bool {
    let re = dangerous_regex();

    re.is_match(cmd)
}

fn is_within_root(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let root = root_or_cwd()?;
    let canonical_root = root.canonicalize()?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };

    // Walk up to the nearest existing ancestor, since `path` itself may not exist yet
    // (e.g., a new file or a directory that will be created by the tool).
    let mut ancestor = candidate.as_path();
    loop {
        if ancestor.exists() {
            let canonical_ancestor = ancestor.canonicalize()?;
            return Ok(canonical_ancestor.starts_with(&canonical_root));
        }
        match ancestor.parent() {
            Some(parent) => ancestor = parent,
            None => return Ok(false),
        }
    }
}

#[derive(Debug)]
pub struct SearchTool;

#[async_trait::async_trait]
impl ToolFunction<()> for SearchTool {
    fn name(&self) -> &'static str {
        "search"
    }

    fn description(&self) -> &'static str {
        "Semantically search across files in the workspace. Returns the most relevant chunks with their document path and similarity score."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural-language query to search for in the indexed files."
                },
                "document_paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of document paths to restrict the search to."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum number of results to return (default 10)."
                },
                "score_threshold": {
                    "type": "number",
                    "description": "Optional minimum similarity score for returned results."
                }
            }
        })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &Arc<ToolExecutionContext<()>>,
    ) -> Result<ToolResult, AgentError> {
        let query = match input["query"].as_str() {
            Some(q) => q.to_string(),
            None => {
                return Ok(ToolResult::Err(
                    "Missing required field 'query' in tool input".into(),
                ));
            }
        };
        let document_paths: Option<Vec<String>> = input["document_paths"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });
        let limit = input["limit"].as_u64().map(|l| l as usize);
        let score_threshold = input["score_threshold"].as_f64().map(|l| l as f32);

        let (embedding, sparse_embedding) = embed_query(&query);
        match vector_search(
            embedding,
            sparse_embedding,
            document_paths,
            limit,
            score_threshold,
        ) {
            Ok(results) => {
                let payload: Vec<Value> = results
                    .into_iter()
                    .map(|r| {
                        json!({
                            "document_path": r.document_path,
                            "content": r.content,
                            "score": r.score,
                        })
                    })
                    .collect();
                Ok(ToolResult::Ok(
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_string()),
                ))
            }
            Err(e) => Ok(ToolResult::Err(format!("Search failed: {e}"))),
        }
    }
}

#[derive(Debug)]
pub struct ReadTool;

#[async_trait::async_trait]
impl ToolFunction<()> for ReadTool {
    fn name(&self) -> &'static str {
        "read"
    }

    fn description(&self) -> &'static str {
        "Read the contents of a file from the local filesystem. This tool also allows to extract the text content unstructured files (PDFs, Office documents, scanned images) through LiteParse."
    }

    fn input_schema(&self) -> Value {
        json!({
          "type": "object",
          "required": ["path"],
          "properties": {
            "path": {
              "type": "string",
              "description": "Path to the file to read."
            },
            "pages": {
              "type": "array",
              "items": {
                "type": "integer",
                "minimum": 0
              },
              "description": "List of page numbers to read (0-based). Only applies to unstructured documents (PDFs, Office docs)."
            },
            "offset": {
              "type": "integer",
              "description": "Number of characters to skip from the start of the file. Only applies to text-based files.",
              "minimum": 0,
              "default": 0
            },
            "max_length": {
              "type": "integer",
              "description": "Maximum number of characters to read. Only applies to text-based files.",
              "minimum": 1
            }
          }
        })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &Arc<ToolExecutionContext<()>>,
    ) -> Result<ToolResult, AgentError> {
        let path = match input["path"].as_str() {
            Some(p) => p,
            None => {
                return Ok(ToolResult::Err(
                    "Missing required field 'path' in tool input".into(),
                ));
            }
        };
        let p = Path::new(path);
        let within_root =
            is_within_root(p).map_err(|e| AgentError::ToolCallError(e.to_string()))?;
        if !within_root {
            return Ok(ToolResult::Err(
                "Path does not appear to be within the current workspace or its root is non-resolvable".into(),
            ));
        }
        if SUPPORTED_LIT_EXTENSIONS.contains(
            &p.extension()
                .unwrap_or_default()
                .to_str()
                .map(|s| format!(".{}", s.to_lowercase()))
                .unwrap_or_default()
                .as_str(),
        ) {
            let _guard = PARSER_MUTEX.get_or_init(|| Mutex::new(())).lock().await;
            let result = parser().parse(path).await;
            match result {
                Ok(content) => {
                    if let Some(pgs) = input["pages"].as_array() {
                        let lit_pages = content.pages;
                        let mut text = String::new();
                        for pg in pgs {
                            if let Some(v) = pg.as_u64()
                                && let Some(p) = lit_pages.get(v as usize)
                            {
                                text += &format!("Page {v}\n\n{}\n\n---\n\n", p.text);
                            }
                        }
                        return Ok(ToolResult::Ok(text));
                    } else {
                        return Ok(ToolResult::Ok(content.text));
                    }
                }
                Err(e) => return Ok(ToolResult::Err(format!("Failed to read {path}: {e}"))),
            }
        }
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let text = content;

                let offset_chars = input["offset"].as_u64().map(|o| o as usize).unwrap_or(0);
                let max_chars = input["max_length"].as_u64().map(|m| m as usize);

                let start = text
                    .char_indices()
                    .nth(offset_chars)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len());
                let text = &text[start..];

                let text = if let Some(max) = max_chars {
                    let end = text
                        .char_indices()
                        .nth(max)
                        .map(|(i, _)| i)
                        .unwrap_or(text.len());
                    &text[..end]
                } else {
                    text
                };

                Ok(ToolResult::Ok(text.to_string()))
            }
            Err(e) => Ok(ToolResult::Err(format!("Failed to read {path}: {e}"))),
        }
    }
}

#[derive(Debug)]
pub struct WriteTool;

#[async_trait::async_trait]
impl ToolFunction<()> for WriteTool {
    fn name(&self) -> &'static str {
        "write"
    }

    fn description(&self) -> &'static str {
        "Write content to a file, creating it (and parent directories) if needed. Overwrites the file if it exists."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write."
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file."
                }
            }
        })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &Arc<ToolExecutionContext<()>>,
    ) -> Result<ToolResult, AgentError> {
        let path = match input["path"].as_str() {
            Some(p) => p,
            None => {
                return Ok(ToolResult::Err(
                    "Missing required field 'path' in tool input".into(),
                ));
            }
        };
        let p = Path::new(path);
        let within_root =
            is_within_root(p).map_err(|e| AgentError::ToolCallError(e.to_string()))?;
        if !within_root {
            return Ok(ToolResult::Err(
                "Path does not appear to be within the current workspace or its root is non-resolvable".into(),
            ));
        }
        let content = match input["content"].as_str() {
            Some(c) => c,
            None => {
                return Ok(ToolResult::Err(
                    "Missing required field 'content' in tool input".into(),
                ));
            }
        };
        if let Some(parent) = Path::new(path).parent()
            && !parent.as_os_str().is_empty()
            && let Err(e) = fs::create_dir_all(parent)
        {
            return Ok(ToolResult::Err(format!(
                "Failed to create parent directories for {path}: {e}"
            )));
        }
        match fs::write(path, content) {
            Ok(_) => Ok(ToolResult::Ok(format!(
                "Wrote {} bytes to {path}",
                content.len()
            ))),
            Err(e) => Ok(ToolResult::Err(format!("Failed to write {path}: {e}"))),
        }
    }
}

#[derive(Debug)]
pub struct EditTool;

#[async_trait::async_trait]
impl ToolFunction<()> for EditTool {
    fn name(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> &'static str {
        "Edit a file by replacing an exact occurrence of `old_str` with `new_str`. The `old_str` must match exactly once in the file."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "old_str", "new_str"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit."
                },
                "old_str": {
                    "type": "string",
                    "description": "Exact string to find and replace. Must occur exactly once."
                },
                "new_str": {
                    "type": "string",
                    "description": "Replacement string."
                }
            }
        })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &Arc<ToolExecutionContext<()>>,
    ) -> Result<ToolResult, AgentError> {
        let path = match input["path"].as_str() {
            Some(p) => p,
            None => {
                return Ok(ToolResult::Err("Missing required field 'path'".into()));
            }
        };
        let p = Path::new(path);
        let within_root =
            is_within_root(p).map_err(|e| AgentError::ToolCallError(e.to_string()))?;
        if !within_root {
            return Ok(ToolResult::Err(
                "Path does not appear to be within the current workspace or its root is non-resolvable".into(),
            ));
        }
        let old_str = match input["old_str"].as_str() {
            Some(s) => s,
            None => {
                return Ok(ToolResult::Err("Missing required field 'old_str'".into()));
            }
        };
        let new_str = match input["new_str"].as_str() {
            Some(s) => s,
            None => {
                return Ok(ToolResult::Err("Missing required field 'new_str'".into()));
            }
        };

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult::Err(format!("Failed to read {path}: {e}"))),
        };
        let occurrences = content.matches(old_str).count();
        if occurrences == 0 {
            return Ok(ToolResult::Err(format!("`old_str` not found in {path}")));
        }
        if occurrences > 1 {
            return Ok(ToolResult::Err(format!(
                "`old_str` matched {occurrences} times in {path}; it must be unique"
            )));
        }
        // Replace
        let updated = content.replacen(old_str, new_str, 1);

        // Atomic write — temp file in same dir so rename stays on same filesystem
        let parent = p.parent().unwrap_or(Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| AgentError::ToolCallError(e.to_string()))?;
        tmp.write_all(updated.as_bytes())
            .map_err(|e| AgentError::ToolCallError(e.to_string()))?;

        // Preserve original permissions/metadata before overwriting
        if p.exists() {
            match fs::metadata(p) {
                Ok(meta) => {
                    #[cfg(unix)]
                    {
                        let mode = meta.permissions().mode();
                        let mut perms = fs::metadata(tmp.path())
                            .map(|m| m.permissions())
                            .unwrap_or_else(|_| std::fs::Permissions::from_mode(mode));
                        perms.set_mode(mode);
                        let _ = fs::set_permissions(tmp.path(), perms);
                    }
                    #[cfg(windows)]
                    {
                        let _ = fs::set_permissions(tmp.path(), meta.permissions());
                    }
                }
                Err(_) => { /* ignore metadata read errors */ }
            }
        }

        tmp.persist(p)
            .map_err(|e| AgentError::ToolCallError(e.to_string()))?;

        Ok(ToolResult::Ok(format!("Edited {path}")))
    }
}

#[derive(Debug)]
pub struct ShellExecuteTool;

#[async_trait::async_trait]
impl ToolFunction<()> for ShellExecuteTool {
    fn name(&self) -> &'static str {
        "shell_execute"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command and return its stdout, stderr, and exit status. Supports Bash-compatible shells and Powershell."
    }

    fn input_schema(&self) -> Value {
        json!({
          "type": "object",
          "required": [
            "command"
          ],
          "properties": {
            "command": {
              "type": "string",
              "description": "Shell command to execute (run via `sh -c`)."
            },
            "cwd": {
              "type": "string",
              "description": "Optional working directory for the command."
            },
            "timeout": {
              "type": "integer",
              "description": "Command execution timeout in seconds (only integers allowed). Defaults to 60 seconds."
            }
          }
        })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &Arc<ToolExecutionContext<()>>,
    ) -> Result<ToolResult, AgentError> {
        let command = match input["command"].as_str() {
            Some(c) => c,
            None => {
                return Ok(ToolResult::Err(
                    "Missing required field 'command' in tool input".into(),
                ));
            }
        };
        if is_dangerous(command) {
            return Ok(ToolResult::Err(
                "Dangerous command (`{command}`) detected and blocked".into(),
            ));
        }

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("powershell");
            c.arg("-NoProfile").arg("-Command").arg(command);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c").arg(command);
            c
        };

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = input["cwd"].as_str() {
            cmd.current_dir(cwd);
        }
        let tim = input["timeout"].as_u64().unwrap_or(60);
        let res = tokio::time::timeout(Duration::from_secs(tim), cmd.output()).await;
        let out = match res {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::Err(format!(
                    "Error while executing {command} with timeout: {e}"
                )));
            }
        };
        match out {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let code = output.status.code();
                let payload = json!({
                    "exit_code": code,
                    "stdout": stdout,
                    "stderr": stderr,
                });
                let body =
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
                if output.status.success() {
                    Ok(ToolResult::Ok(body))
                } else {
                    Ok(ToolResult::Err(body))
                }
            }
            Err(e) => Ok(ToolResult::Err(format!(
                "Failed to spawn command `{command}`: {e}"
            ))),
        }
    }
}
