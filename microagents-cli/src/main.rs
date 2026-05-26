use microagents_core::{
    agent::MicroAgentBuilder,
    types::{Agent, AgentError, ToolExecutionContext, ToolFunction},
};
use microagents_events::types::ToolResult;
use microagents_storage::types::AgentStorageChoice;
use serde_json::{Value, json};
use std::sync::Arc;

mod init_env;
mod tools;
mod tui;

#[derive(Debug)]
struct WeatherTool;

#[async_trait::async_trait]
impl ToolFunction<()> for WeatherTool {
    fn name(&self) -> String {
        "weather_tool".into()
    }

    fn description(&self) -> String {
        "Get the weather for a give location".into()
    }

    fn input_schema(&self) -> Value {
        json!({
          "type": "object",
          "required": [
            "location"
          ],
          "properties": {
            "location": {
              "type": "string",
              "description": "Location to get the weather for"
            }
          }
        })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &Arc<ToolExecutionContext<()>>,
    ) -> Result<ToolResult, AgentError> {
        let location = input["location"].as_str();
        if let Some(l) = location {
            return Ok(ToolResult::Ok(format!(
                "Weather at {l} is sunny with 27 degrees Celsius."
            )));
        }
        Ok(ToolResult::Err(
            "Could not find location in the tool input".into(),
        ))
    }
}

fn build_agent() -> Result<microagents_core::agent::MicroAgent<()>, AgentError> {
    Ok(MicroAgentBuilder::<()>::new(ToolExecutionContext::new(()))
        .model("anthropic/claude-opus-4.7".into())
        .provider("openrouter".into())
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .storage(AgentStorageChoice::Jsonl)
        .custom_instructions(
            "Always use the weather_tool to get the weather of a location".into(),
        )
        .add_tool(Arc::new(WeatherTool))
        .map_err(|e| AgentError::ClientInitFailed(e.to_string()))?
        .build())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tui::run(|prompt, session_id| async move {
        let agent = build_agent()?;
        agent.run(prompt, session_id).await
    })
    .await?;
    Ok(())
}
