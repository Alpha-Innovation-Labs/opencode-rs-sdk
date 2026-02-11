//! Example: Clean (delete) conversations matching a pattern with interactive confirmation.
//!
//! Run with: cargo run --example oc_clean_conversations --features server -- [pattern]
//! Default pattern: "openagora" (case-insensitive)
//! Always shows dry-run first, then asks for confirmation before deleting.
//! This example starts a managed OpenCode server automatically.

use opencode_rs::runtime::ManagedRuntime;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Parse arguments
    let pattern = args.get(1).cloned().unwrap_or_else(|| "openagora".to_string());

    let runtime = ManagedRuntime::start_for_cwd().await?;
    println!(
        "Managed runtime ready at: http://localhost:{}",
        runtime.server().port()
    );
    let client = runtime.client();

    // List all sessions
    let sessions = client.sessions().list().await?;

    // Filter sessions by title (case-insensitive)
    let matching_sessions: Vec<_> = sessions
        .iter()
        .filter(|s| s.title.to_lowercase().contains(&pattern.to_lowercase()))
        .collect();

    if matching_sessions.is_empty() {
        println!(
            "\n\x1b[1;33m⚠️  No sessions found matching pattern: '{}'\x1b[0m",
            pattern
        );
        println!(
            "\x1b[90m   Total sessions checked: {}\x1b[0m\n",
            sessions.len()
        );
        return Ok(());
    }

    // Show dry-run preview (titles only)
    println!(
        "\n\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!("\x1b[1;33m🧹 DRY RUN - Conversations to be Deleted\x1b[0m");
    println!("\x1b[90m   Pattern: '{}'\x1b[0m", pattern);
    println!(
        "\x1b[90m   Found: {} conversation(s)\x1b[0m",
        matching_sessions.len()
    );
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m\n"
    );

    println!("\x1b[1;35mConversations that will be removed:\x1b[0m\n");
    for (i, session) in matching_sessions.iter().enumerate() {
        println!("  \x1b[33m{}.\x1b[0m {}", i + 1, session.title);
    }

    println!(
        "\n\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!("\x1b[90m   Total to delete: {} conversation(s)\x1b[0m", matching_sessions.len());
    println!("\x1b[90m   This is a preview - no deletions performed yet\x1b[0m");
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m\n"
    );

    // Ask for confirmation
    println!("\x1b[1;33mDo you want to proceed with deletion? [y/N]: \x1b[0m");
    
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let confirmed = input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes";

    if !confirmed {
        println!("\n\x1b[1;33m⚠️  Cancelled. No conversations were deleted.\x1b[0m\n");
        return Ok(());
    }

    // Proceed with deletion
    println!(
        "\n\x1b[1;33m🗑️  Deleting {} conversation(s)...\x1b[0m\n",
        matching_sessions.len()
    );

    let mut deleted = 0;
    let mut failed = 0;

    for session in &matching_sessions {
        print!("  Deleting '{}'... ", session.title);
        match client.sessions().delete(&session.id).await {
            Ok(_) => {
                println!("\x1b[32m✓\x1b[0m");
                deleted += 1;
            }
            Err(e) => {
                println!("\x1b[31m✗ {}\x1b[0m", e);
                failed += 1;
            }
        }
    }

    println!();
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!("\x1b[1;33m📊 Summary\x1b[0m");
    println!("\x1b[32m   Deleted: {} conversation(s)\x1b[0m", deleted);
    if failed > 0 {
        println!("\x1b[31m   Failed: {} conversation(s)\x1b[0m", failed);
    }
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m\n"
    );

    Ok(())
}
