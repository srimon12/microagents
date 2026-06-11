use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RelevantChunk {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub details: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Question {
    pub query: String,
    pub relevant_chunks: Vec<RelevantChunk>,
}

pub fn load_questions(questions_file: &str) -> anyhow::Result<Vec<Question>> {
    let content = fs::read_to_string(questions_file)?;
    let questions: Vec<Question> = serde_json::from_str(&content)?;
    Ok(questions)
}
