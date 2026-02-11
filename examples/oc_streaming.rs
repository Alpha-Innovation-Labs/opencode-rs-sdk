//! Streaming test with two session-specific subscriptions.
//!
//! Run with: cargo run --example oc_streaming --features full

use opencode_rs::runtime::ManagedRuntime;
use opencode_rs::types::event::{Event, SessionErrorProps};
use opencode_rs::types::message::Part;
use opencode_rs::types::message::{PromptPart, PromptRequest};
use opencode_rs::types::project::ModelRef;
use opencode_rs::types::session::CreateSessionRequest;
use std::time::Duration;

fn is_status_idle(properties: &serde_json::Value) -> bool {
    properties
        .get("status")
        .and_then(|s| s.get("type"))
        .and_then(|v| v.as_str())
        == Some("idle")
}

fn format_session_error(properties: &SessionErrorProps) -> String {
    match &properties.error {
        Some(err) => format!("{err:?}"),
        None => "unknown session error".to_string(),
    }
}

async fn collect_stream(
    mut sub: opencode_rs::sse::SseSubscription,
    label: &str,
    session_id: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let started = tokio::time::Instant::now();
    let window = Duration::from_secs(180);
    let mut output = String::new();

    println!(
        "[{}] collecting filtered stream for session {} ({}s timeout)",
        label,
        session_id,
        window.as_secs()
    );
    loop {
        if started.elapsed() >= window {
            return Err(format!("{} timed out waiting for idle", label).into());
        }

        let event = tokio::time::timeout(Duration::from_secs(5), sub.recv()).await;
        match event {
            Ok(Some(e)) => match e {
                Event::MessagePartUpdated { properties } => {
                    if let Some(delta) = properties.delta.as_deref() {
                        if !delta.trim().is_empty() {
                            println!("[{}][message.part.updated] {}", label, delta);
                            output.push_str(delta);
                        }
                    }
                }
                Event::SessionStatus { properties } => {
                    if is_status_idle(&properties) {
                        println!("[{}][session.status] idle", label);
                        break;
                    }
                }
                Event::SessionIdle { .. } => {
                    println!("[{}][session.idle]", label);
                    break;
                }
                Event::SessionError { properties } => {
                    let err_text = format_session_error(&properties);
                    println!("[{}][session.error] {}", label, err_text);
                    return Err(format!("{} failed: {}", label, err_text).into());
                }
                _ => {}
            },
            Ok(None) => {
                return Err(format!("{} stream closed before completion", label).into());
            }
            Err(_) => {
                println!("[{}] no event in last 5s", label);
            }
        }
    }

    Ok(output.trim().to_string())
}

fn latest_assistant_text(msgs: &[opencode_rs::types::message::Message]) -> String {
    for msg in msgs.iter().rev() {
        if msg.info.role != "assistant" {
            continue;
        }

        let mut out = String::new();
        for part in &msg.parts {
            if let Part::Text { text, .. } = part {
                if !text.trim().is_empty() {
                    out.push_str(text);
                }
            }
        }
        if !out.trim().is_empty() {
            return out.trim().to_string();
        }
    }
    String::new()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Starting managed runtime...");
    let runtime = ManagedRuntime::start_for_cwd().await?;
    println!(
        "Managed runtime ready at: http://localhost:{}",
        runtime.server().port()
    );
    let client = runtime.client();

    println!("Creating 2 sessions...");
    let s1 = client
        .sessions()
        .create(&CreateSessionRequest {
            title: Some("oc_streaming_1".into()),
            ..Default::default()
        })
        .await?;
    let s2 = client
        .sessions()
        .create(&CreateSessionRequest {
            title: Some("oc_streaming_2".into()),
            ..Default::default()
        })
        .await?;

    println!("Session 1: {}", s1.id);
    println!("Session 2: {}", s2.id);

    println!("Subscribing to SSE stream for each session...");
    let sub1 = client.subscribe_session(&s1.id).await?;
    let sub2 = client.subscribe_session(&s2.id).await?;

    let stream1 = collect_stream(sub1, "session1", &s1.id);
    let stream2 = collect_stream(sub2, "session2", &s2.id);

    let req_en = PromptRequest {
        parts: vec![PromptPart::Text {
            text: "give me a joke".to_string(),
            synthetic: None,
            ignored: None,
            metadata: None,
        }],
        message_id: None,
        model: Some(ModelRef {
            provider_id: "opencode".to_string(),
            model_id: "kimi-k2.5-free".to_string(),
        }),
        agent: None,
        no_reply: None,
        system: None,
        variant: None,
    };

    let req_fr = PromptRequest {
        parts: vec![PromptPart::Text {
            text: "raconte-moi une blague".to_string(),
            synthetic: None,
            ignored: None,
            metadata: None,
        }],
        message_id: None,
        model: Some(ModelRef {
            provider_id: "opencode".to_string(),
            model_id: "kimi-k2.5-free".to_string(),
        }),
        agent: None,
        no_reply: None,
        system: None,
        variant: None,
    };

    println!("Sending prompts to both sessions...");
    println!("  Session 1 -> English");
    println!("  Session 2 -> French");
    client.messages().prompt_async(&s1.id, &req_en).await?;
    client.messages().prompt_async(&s2.id, &req_fr).await?;

    println!("Waiting for both filtered streams to finish...");
    let (result1, result2) = tokio::join!(stream1, stream2);
    let result1 = result1?;
    let result2 = result2?;

    let msgs1 = client.messages().list(&s1.id).await?;
    let msgs2 = client.messages().list(&s2.id).await?;
    let latest1 = latest_assistant_text(&msgs1);
    let latest2 = latest_assistant_text(&msgs2);

    println!("\n===== DONE =====");
    println!("Session 1: {}", s1.id);
    println!("Session 2: {}", s2.id);
    println!("\n===== STREAM RESULTS =====");
    println!(
        "Session 1 ({}):\n{}",
        s1.id,
        if result1.is_empty() {
            "[empty]"
        } else {
            &result1
        }
    );
    println!(
        "\nSession 2 ({}):\n{}",
        s2.id,
        if result2.is_empty() {
            "[empty]"
        } else {
            &result2
        }
    );

    println!("\n===== MESSAGE LIST RESULTS =====");
    println!(
        "Session 1 ({}):\n{}",
        s1.id,
        if latest1.is_empty() {
            "[empty]"
        } else {
            &latest1
        }
    );
    println!(
        "\nSession 2 ({}):\n{}",
        s2.id,
        if latest2.is_empty() {
            "[empty]"
        } else {
            &latest2
        }
    );

    use std::io::{self, Write};
    println!("\nPress Enter to stop runtime and exit...");
    print!("> ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    runtime.stop().await?;
    Ok(())
}
