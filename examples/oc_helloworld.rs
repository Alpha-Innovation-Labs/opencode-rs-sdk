//! Hello World example using managed runtime.
//!
//! Run with: cargo run --example oc_helloworld --features server

use opencode_rs::runtime::ManagedRuntime;
use opencode_rs::types::message::Part;
use opencode_rs::types::project::ModelRef;
use opencode_rs::types::session::SessionCreateOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting managed runtime...");
    let runtime = ManagedRuntime::start_for_cwd().await?;
    println!(
        "Managed runtime ready at: http://localhost:{}",
        runtime.server().port()
    );
    let client = runtime.client();

    println!("Creating session...");
    let session = client
        .sessions()
        .create_with(SessionCreateOptions::new().with_title("Hello World"))
        .await?;
    let session_id = session.id;

    println!("Sending prompt...");
    client
        .messages()
        .send_text_async(
            &session_id,
            "Tell me a short joke.",
            Some(ModelRef {
                provider_id: "opencode".to_string(),
                model_id: "kimi-k2.5-free".to_string(),
            }),
        )
        .await?;

    println!("Waiting for reply...");
    let mut joke = String::new();
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let msgs = client.messages().list(&session_id).await?;
        for msg in msgs.iter().rev() {
            if msg.info.role != "assistant" {
                continue;
            }
            for part in &msg.parts {
                if let Part::Text { text, .. } = part {
                    if !text.trim().is_empty() {
                        joke = text.clone();
                        break;
                    }
                }
            }
            if !joke.is_empty() {
                break;
            }
        }
        if !joke.is_empty() {
            break;
        }
    }

    println!("Session: {}", session_id);
    if joke.is_empty() {
        println!("No assistant reply yet. Check the session in OpenCode.");
    } else {
        println!("Joke:\n{}", joke);
    }

    runtime.stop().await?;
    Ok(())
}
