# microagents-events

Event types and JSON-RPC serialization for the [microagents](https://github.com/AstraBert/microagents) framework.

## Overview

This crate defines the core event types that flow through an agent session: initialization, user prompts, streaming deltas, tool calls, skill loading and assistant responses. Each event can be converted to and from a JSON-RPC 2.0 notification for transport over a wire or between processes.

## Event Types

| Event | Method | Description |
|-------|--------|-------------|
| `SessionInitEvent` | `session.init` | A new or resumed session starts |
| `SessionStopEvent` | `session.stop` | A session ends with success or error |
| `UserPromptSubmitEvent` | `user.prompt.submit` | The user sends a prompt |
| `StreamDeltaEvent` | `stream.delta` | A chunk of streaming response (text or thinking) |
| `ToolCallEvent` | `tool.call` | The model requests a tool invocation |
| `ToolResultEvent` | `tool.result` | A tool returns its output |
| `SkillLoadEvent` | `skill.load` | A skill is loaded into the session |
| `AssistantResponseEvent` | `assistant.response` | The model produces a complete response |

## Usage

```rust
use microagents_events::*;

let event = SessionInitEvent {
    session_id: "sess-1".into(),
    model: "gpt-4".into(),
    provider: "openai".into(),
    system: "You are a helpful assistant.".into(),
    init_type: SessionInitType::Start,
};

let rpc = event.to_jsonrpc();
// rpc.method == "session.init"
// rpc.params["session_id"] == "sess-1"
```

## Round-trip

```rust
let rpc = JsonRpcNotification::builder()
    .method("user.prompt.submit".into())
    .add_param("session_id".into(), "s1".into())
    .add_param("turn_id".into(), "t1".into())
    .add_param("prompt".into(), "Hello!".into());

let any = AgentEventAny::try_from(rpc)?;
```

## License

MIT
