//! Example: Get full conversation history for a session.
//!
//! Run with: cargo run --example oc_get_conversation -- <session_id>
//!
//! Requires an OpenCode server running at localhost:4096:
//!   opencode serve

use opencode_rs::ClientBuilder;
use opencode_rs::types::message::{Message, Part, ToolState};
use opencode_rs::types::session::Session;

/// Format duration in milliseconds to human-readable format
fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        return format!("{}ms", ms);
    }

    let seconds = ms / 1000;
    let remaining_ms = ms % 1000;

    if seconds < 60 {
        return format!("{}.{:03}s", seconds, remaining_ms);
    }

    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;

    if minutes < 60 {
        return format!("{}m {}s", minutes, remaining_seconds);
    }

    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    format!("{}h {}m {}s", hours, remaining_minutes, remaining_seconds)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: cargo run --example oc_get_conversation -- <session_id>");
        std::process::exit(1);
    }

    let session_id = &args[1];

    // Get current directory
    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();

    // Build client with directory context
    let client = ClientBuilder::new().directory(current_dir).build()?;

    // Get session details and messages in parallel
    let session = client.sessions().get(session_id).await?;
    let messages = client.messages().list(session_id).await?;

    // Format and display output
    format_output(&session, &messages);

    Ok(())
}

fn format_output(session: &Session, messages: &[Message]) {
    // Calculate total conversation time
    let total_duration_ms = session
        .time
        .as_ref()
        .map(|t| t.updated - t.created)
        .unwrap_or(0);

    // Header
    println!(
        "\n\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!("\x1b[1;33mSession:\x1b[0m {}", session.title);
    println!("\x1b[1;33mID:\x1b[0m {}", session.id);
    println!(
        "\x1b[1;33mDirectory:\x1b[0m {}",
        session.directory.as_deref().unwrap_or("Unknown")
    );
    println!("\x1b[1;33mVersion:\x1b[0m {}", session.version);
    if let Some(time) = &session.time {
        println!("\x1b[1;33mCreated:\x1b[0m {}", time.created);
        println!("\x1b[1;33mUpdated:\x1b[0m {}", time.updated);
    }
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );

    // Messages
    println!("\n\x1b[1;35m💬 Messages:\x1b[0m\n");

    for msg in messages {
        let role = msg.info.role.to_uppercase();
        print!("\n\x1b[1;33m[{}]\x1b[0m", role);
        print!(" \x1b[90m{}\x1b[0m", msg.info.time.created);

        // Show duration if message is completed (human readable)
        if let Some(completed) = msg.info.time.completed {
            let duration_ms = completed - msg.info.time.created;
            print!(" \x1b[90m({})\x1b[0m", format_duration(duration_ms));
        }
        println!();

        for part in &msg.parts {
            match part {
                Part::Text { text, .. } => {
                    println!("{}", text);
                }
                Part::Tool {
                    tool, state, input, ..
                } => {
                    println!("\x1b[90m[Tool: {}]\x1b[0m", tool);
                    // Display tool input if available
                    if let Some(input_val) = input.get("input").and_then(|v| v.as_str()) {
                        println!("{}", input_val);
                    }
                    // Display tool state/output if available
                    if let Some(tool_state) = state {
                        match tool_state {
                            ToolState::Completed(completed) => {
                                println!("\x1b[90mOutput: {}\x1b[0m", completed.output);
                            }
                            ToolState::Error(err) => {
                                println!("\x1b[90mError: {}\x1b[0m", err.error);
                            }
                            _ => {}
                        }
                    }
                }
                Part::StepFinish {
                    cost,
                    tokens,
                    reason,
                    ..
                } => {
                    print!("\x1b[90m[Step: {} | cost: ${:.4}]\x1b[0m", reason, cost);
                    if let Some(tok) = tokens {
                        print!(" \x1b[90m[Tokens: in={} out={}", tok.input, tok.output);
                        if tok.reasoning > 0 {
                            print!(" reasoning={}", tok.reasoning);
                        }
                        print!("]\x1b[0m");
                    }
                    println!();
                }
                _ => {} // Skip other part types
            }
        }
    }

    // Total conversation time at the end
    println!();
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!(
        "\x1b[1;35m⏱️  Total Conversation Time: {}\x1b[0m",
        format_duration(total_duration_ms)
    );
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!();
}
