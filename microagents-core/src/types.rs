use schemars::JsonSchema;

#[derive(Debug)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

impl Message {
    pub fn new(role: MessageRole, content: String) -> Self {
        Self { role, content }
    }
}

#[async_trait::async_trait]
pub trait LLM {
    async fn generate(&self, history: Vec<Message>, schema: impl JsonSchema);
}
