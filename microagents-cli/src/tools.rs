use microagents_core::types::{AgentError, ToolExecutionContext, ToolFunction};
use microagents_events::types::ToolResult;
use regex::Regex;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, OnceLock};

use tokio::process::Command;

use crate::init_env::{SUPPORTED_LIT_EXTENSIONS, embed_query, parser};
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

#[derive(Debug)]
pub struct SearchTool;

#[async_trait::async_trait]
impl ToolFunction<()> for SearchTool {
    fn name(&self) -> String {
        "search".into()
    }

    fn description(&self) -> String {
        "Semantically search across files in the workspace. Returns the most relevant chunks with their document path and similarity score.".into()
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
        let limit = input["limit"].as_u64().map(|l| l as u32);
        let score_threshold = input["score_threshold"].as_f64();

        let embedding = embed_query(&query);
        match vector_search(embedding, document_paths, limit, score_threshold) {
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
    fn name(&self) -> String {
        "read".into()
    }

    fn description(&self) -> String {
        "Read the contents of a file from the local filesystem. This tool also allows to extract the text content unstructured files (PDFs, Office documents, scanned images) through LiteParse.".into()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read."
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
        if SUPPORTED_LIT_EXTENSIONS.contains(
            &p.extension()
                .unwrap_or_default()
                .to_str()
                .map(|s| format!(".{}", s.to_lowercase()))
                .unwrap_or_default()
                .as_str(),
        ) {
            let result = parser().parse(path).await;
            match result {
                Ok(content) => return Ok(ToolResult::Ok(content.text)),
                Err(e) => return Ok(ToolResult::Err(format!("Failed to read {path}: {e}"))),
            }
        }
        match fs::read_to_string(path) {
            Ok(content) => Ok(ToolResult::Ok(content)),
            Err(e) => Ok(ToolResult::Err(format!("Failed to read {path}: {e}"))),
        }
    }
}

#[derive(Debug)]
pub struct WriteTool;

#[async_trait::async_trait]
impl ToolFunction<()> for WriteTool {
    fn name(&self) -> String {
        "write".into()
    }

    fn description(&self) -> String {
        "Write content to a file, creating it (and parent directories) if needed. Overwrites the file if it exists.".into()
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
        let content = match input["content"].as_str() {
            Some(c) => c,
            None => {
                return Ok(ToolResult::Err(
                    "Missing required field 'content' in tool input".into(),
                ));
            }
        };
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return Ok(ToolResult::Err(format!(
                        "Failed to create parent directories for {path}: {e}"
                    )));
                }
            }
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
    fn name(&self) -> String {
        "edit".into()
    }

    fn description(&self) -> String {
        "Edit a file by replacing an exact occurrence of `old_str` with `new_str`. The `old_str` must match exactly once in the file.".into()
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
        let updated = content.replacen(old_str, new_str, 1);
        match fs::write(path, updated) {
            Ok(_) => Ok(ToolResult::Ok(format!("Edited {path}"))),
            Err(e) => Ok(ToolResult::Err(format!("Failed to write {path}: {e}"))),
        }
    }
}

#[derive(Debug)]
pub struct ShellExecuteTool;

#[async_trait::async_trait]
impl ToolFunction<()> for ShellExecuteTool {
    fn name(&self) -> String {
        "shell_execute".into()
    }

    fn description(&self) -> String {
        "Execute a shell command and return its stdout, stderr, and exit status. Supports Bash-compatible shells and Powershell.".into()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute (run via `sh -c`)."
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory for the command."
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

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(cwd) = input["cwd"].as_str() {
            cmd.current_dir(cwd);
        }
        match cmd.output().await {
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
