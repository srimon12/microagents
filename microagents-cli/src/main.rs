use futures_util::StreamExt;
use microagents_core::{
    agent::MicroAgentBuilder,
    types::{Agent, AgentError, ToolExecutionContext, ToolFunction},
};
use microagents_events::types::{AgentEvent, ToolResult};
use serde_json::{Value, json};
use std::sync::Arc;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let agent = MicroAgentBuilder::<()>::new(ToolExecutionContext::new(()))
        .model("anthropic/claude-opus-4.7".into())
        .provider("openrouter".into())?
        .custom_instructions("Always use the weather_tool to get the weather of a location".into())
        .add_tool(Arc::new(WeatherTool))?
        .build();
    let mut run = agent
        .run(
            "What is the weather in San Francisco? Call the weather_tool to see".into(),
            None,
        )
        .await?;
    while let Some(event) = run.next().await {
        match event {
            Ok(agent_event) => {
                let json_rpc = agent_event.to_jsonrpc();
                let s = serde_json::to_string(&json_rpc)?;
                println!("{}", s);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }
    Ok(())
}
