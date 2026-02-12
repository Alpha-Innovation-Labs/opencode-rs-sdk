//! Example: List all OpenCode sessions for the current directory.
//!
//! Run with: cargo run --example oc_list_sessions --features server
//!
//! This example starts a managed OpenCode server automatically.

use opencode_sdk::runtime::ManagedRuntime;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = ManagedRuntime::start_for_cwd().await?;
    println!(
        "Managed runtime ready at: http://localhost:{}",
        runtime.server().port()
    );
    let client = runtime.client();

    // List all sessions
    let sessions = client.sessions().list().await?;

    if sessions.is_empty() {
        println!("[]");
        return Ok(());
    }

    // Output as JSON for parsing
    let json = serde_json::to_string(&sessions)?;
    println!("{}", json);

    Ok(())
}
