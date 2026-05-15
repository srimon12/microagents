use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestAssistantMessageContentPart,
    ChatCompletionRequestDeveloperMessageContent, ChatCompletionRequestDeveloperMessageContentPart,
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessageContent,
    ChatCompletionRequestUserMessageContentPart,
};
use thiserror::Error;

use crate::types::{LLM, Message, MessageRole};

#[derive(Debug, Error)]
pub enum MessageConversionError {
    #[error("An invalid role has been encountered: {0}")]
    InvalidRoleError(String),
    #[error("Message part deserialization failed")]
    MessagePartDeserialiationError(#[from] serde_json::error::Error),
}

fn from_completion_messages(
    messages: Vec<ChatCompletionRequestMessage>,
) -> Result<Vec<Message>, MessageConversionError> {
    let mut converted: Vec<Message> = vec![];
    for message in &messages {
        match message {
            ChatCompletionRequestMessage::Developer(m) => {
                let content = match m.content {
                    ChatCompletionRequestDeveloperMessageContent::Text(t) => t,
                    ChatCompletionRequestDeveloperMessageContent::Array(parts) => {
                        let mut text = String::new();
                        for part in parts {
                            match part {
                                ChatCompletionRequestDeveloperMessageContentPart::Text(p) => {
                                    text += &format!("{}\n", p.text);
                                }
                            }
                        }
                        text
                    }
                };
                converted.push(Message::new(MessageRole::System, content));
            }
            ChatCompletionRequestMessage::Assistant(m) => {
                let content;
                if let Some(message_content) = m.content {
                    let full_text = match message_content {
                        ChatCompletionRequestAssistantMessageContent::Text(t) => t,
                        ChatCompletionRequestAssistantMessageContent::Array(parts) => {
                            let mut text = String::new();
                            for part in parts {
                                match part {
                                    ChatCompletionRequestAssistantMessageContentPart::Text(s) => {
                                        text += &format!("{}\n", s.text);
                                    }
                                    ChatCompletionRequestAssistantMessageContentPart::Refusal(
                                        r,
                                    ) => {
                                        text += &format!(
                                            "Assistant refused to respond: {}\n",
                                            r.refusal
                                        );
                                    }
                                }
                            }
                            text
                        }
                    };
                    content = full_text;
                } else {
                    content = String::new();
                }
                converted.push(Message::new(MessageRole::Assistant, content));
            }
            ChatCompletionRequestMessage::User(m) => {
                let content = match m.content {
                    ChatCompletionRequestUserMessageContent::Text(t) => t,
                    ChatCompletionRequestUserMessageContent::Array(parts) => {
                        let mut text = String::new();
                        for part in parts {
                            match part {
                                ChatCompletionRequestUserMessageContentPart::Text(t) => {
                                    text += &format!("{}\n", t.text);
                                }
                                ChatCompletionRequestUserMessageContentPart::File(f) => {
                                    let fl = serde_json::to_string(&f.file)?;
                                    text += &format!("\n\n<file>\n{}\n</file>\n\n", fl);
                                }
                                ChatCompletionRequestUserMessageContentPart::ImageUrl(u) => {
                                    text +=
                                        &format!("\n\n<image>\n{}\n</image>\n\n", u.image_url.url);
                                }
                                _ => {}
                            }
                        }
                        text
                    }
                };
            }
        }
    }
    Ok(vec![])
}
