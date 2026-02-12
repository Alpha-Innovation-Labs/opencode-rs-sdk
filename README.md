# opencode-sdk

Rust SDK for OpenCode HTTP API with SSE streaming support. Provides ergonomic async client, 15 REST API modules, 40+ event types, and managed server lifecycle.

## Requirements

- **Platform:** Unix only (Linux/macOS). Windows compilation fails with `compile_error!`.
- **Rust Edition:** 2024 (requires Rust 1.85+)
- **Runtime:** Tokio

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
opencode-sdk = "0.1"

# Or with all features:
opencode-sdk = { version = "0.1", features = ["full"] }
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `http` | ✓ | HTTP API client and modules |
| `sse` | ✓ | SSE streaming support |
| `server` | ✗ | Managed server process |
| `cli` | ✗ | CLI wrapper |
| `full` | ✗ | All features enabled |

## Quick Start

```rust
use opencode_sdk::Client;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client
    let client = Client::builder()
        .base_url("http://127.0.0.1:4096")
        .directory("/path/to/project")
        .timeout_secs(300)
        .build()?;
    
    // Send a simple text prompt and wait for response
    let session = client.run_simple_text("Hello, AI!").await?;
    let response = client.wait_for_idle_text(&session.id, Duration::from_secs(60)).await?;
    println!("Response: {}", response);
    
    Ok(())
}
```

## Streaming Events

```rust
use opencode_sdk::types::Event;

// Subscribe to session events
let mut subscription = client.subscribe_session(&session.id).await?;

while let Some(event) = subscription.recv().await {
    match event {
        Event::MessagePartUpdated { properties } => {
            print!("{}", properties.delta.unwrap_or_default());
        }
        Event::SessionIdle { .. } => break,
        _ => {}
    }
}
```

## API Overview

### Sessions API

```rust
let sessions = client.sessions();

// Create session
let session = sessions.create(&CreateSessionRequest::default()).await?;

// Create with title
let session = sessions.create_with(
    SessionCreateOptions::new().with_title("My Task")
).await?;

// List, get, delete
let all_sessions = sessions.list().await?;
let session = sessions.get(&id).await?;
sessions.delete(&id).await?;

// Fork, share, revert
let forked = sessions.fork(&id).await?;
sessions.share(&id).await?;
sessions.revert(&id, &RevertRequest { message_id, part_id }).await?;
```

### Messages API

```rust
let messages = client.messages();

// Send prompt
let response = messages.prompt(&session_id, &PromptRequest::text("Hello")).await?;

// Send async (use with SSE)
messages.send_text_async(&session_id, "Hello", None).await?;

// List and get messages
let message_list = messages.list(&session_id).await?;
let message = messages.get(&session_id, &message_id).await?;
```

### SSE Subscriptions

```rust
// All events for directory
let sub = client.subscribe().await?;

// Filtered to specific session
let sub = client.subscribe_session(&session_id).await?;

// Global events (all directories)
let sub = client.subscribe_global().await?;

// Raw JSON frames
let raw = client.subscribe_raw().await?;
```

## Managed Server (requires `server` feature)

```rust
use opencode_sdk::server::{ManagedServer, ServerOptions};

// Start managed server
let server = ManagedServer::start(
    ServerOptions::new().port(8080)
).await?;

// Create client connected to managed server
let client = Client::builder()
    .base_url(server.url())
    .build()?;

// Server auto-stops when dropped
server.stop().await?;
```

## CLI Runner (requires `cli` feature)

```rust
use opencode_sdk::cli::{CliRunner, RunOptions};

let mut runner = CliRunner::start(
    "Hello",
    RunOptions::new().model("provider/model")
).await?;

// Stream events
while let Some(event) = runner.recv().await {
    if event.is_text() {
        print!("{}", event.text());
    }
}

// Or collect all text
let text = runner.collect_text().await;
```

## Event Types

The SDK supports 40+ SSE event types organized by category:

- **Server/Instance:** `ServerConnected`, `ServerHeartbeat`, `ServerInstanceDisposed`, `GlobalDisposed`
- **Session:** `SessionCreated`, `SessionUpdated`, `SessionDeleted`, `SessionDiff`, `SessionError`, `SessionCompacted`, `SessionStatus`, `SessionIdle`
- **Messages:** `MessageUpdated`, `MessageRemoved`, `MessagePartUpdated`, `MessagePartRemoved`
- **Questions:** `QuestionCreated`, `QuestionUpdated`, `QuestionRemoved`
- **Files:** `FileChanged`, `FileSynced`
- **MCP:** `MCPServerConnected`, `MCPServerDisconnected`, `MCPError`, `MCPLog`
- **Tool:** `ToolCallCreated`, `ToolCallUpdated`
- **And more...**

## Project Structure

```
src/
├── lib.rs           # Crate root and re-exports
├── client.rs        # Client, ClientBuilder
├── error.rs         # OpencodeError, Result
├── sse.rs           # SSE streaming and subscriptions
├── server.rs        # ManagedServer (server feature)
├── cli.rs           # CliRunner (cli feature)
├── runtime.rs       # ManagedRuntime
├── http/            # HTTP API modules
│   ├── sessions.rs
│   ├── messages.rs
│   └── ...
└── types/           # Data models
    ├── session.rs
    ├── message.rs
    └── event.rs
```

## Error Handling

```rust
match result {
    Err(e) if e.is_not_found() => println!("Not found"),
    Err(e) if e.is_validation_error() => println!("Validation error"),
    Err(e) => eprintln!("Error: {}", e),
    Ok(v) => v,
}
```

## Common Pitfalls

1. **Subscribe before sending** - Create SSE subscription before async prompts to avoid missing early events
2. **Check feature flags** - `server` and `cli` require explicit enabling
3. **Keep server alive** - `ManagedServer` kills on drop; hold reference while using
4. **Handle platform** - Unix only; use WSL or Docker on Windows

## License

Apache-2.0

## Resources

- [API Documentation](https://docs.rs/opencode-rs-sdk)
- [OpenCode Platform](https://opencode.ai)

---

*Version 0.1.3*
