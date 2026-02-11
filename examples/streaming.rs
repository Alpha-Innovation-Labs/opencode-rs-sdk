//! Streaming example showing how to subscribe to SSE events.
//!
//! Run with: cargo run --example streaming
//!
//! Requires an OpenCode server running at localhost:4096:
//!   opencode serve

use opencode_rs::ClientBuilder;
use opencode_rs::types::event::Event;
use opencode_rs::types::message::{PromptPart, PromptRequest};
use opencode_rs::types::project::ModelRef;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt::init();

    // Build client
    let client = ClientBuilder::new().build()?;

    // Use existing session
    let session_id = "ses_3b94a59d7ffe4NqJsbEWC0mIeL";
    println!("Using existing session: {}", session_id);

    // Subscribe to session events BEFORE sending prompt
    let mut subscription = client.subscribe_session(session_id).await?;
    println!("Subscribed to events");

    // Send prompt
    client
        .messages()
        .prompt(
            session_id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Write a haiku about Rust programming".into(),
                    synthetic: None,
                    ignored: None,
                    metadata: None,
                }],
                message_id: None,
                model: Some(ModelRef {
                    provider_id: "kimi-for-coding".into(),
                    model_id: "kimi-k2-thinking".into(),
                }),
                agent: None,
                no_reply: None,
                system: None,
                variant: None,
            },
        )
        .await?;
    println!("Prompt sent, streaming events...\n");

    // Stream events until session is idle or error
    loop {
        match subscription.recv().await {
            Some(Event::SessionIdle { .. }) => {
                println!("\n[Session completed]");
                break;
            }
            Some(Event::SessionError { properties }) => {
                eprintln!("\n[Session error: {:?}]", properties.error);
                break;
            }
            Some(Event::MessagePartUpdated { properties }) => {
                if let Some(delta) = &properties.delta {
                    print!("{}", delta);
                }
            }
            Some(Event::ServerHeartbeat { .. }) => {
                // Heartbeat received, connection alive
            }
            Some(event) => {
                eprintln!("[Other event: {:?}]", event);
            }
            None => {
                println!("[Stream closed]");
                break;
            }
        }
    }

    println!("\nDone!");

    Ok(())
}
