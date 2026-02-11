//! Example: List all projects (unique directories) and their conversations.
//!
//! Run with: cargo run --example oc_list_projects --features server
//!
//! This example starts a managed OpenCode server automatically.

use chrono::{DateTime, Local, TimeZone, Utc};
use opencode_rs::runtime::ManagedRuntime;
use opencode_rs::types::project::Project;
use std::collections::{HashMap, HashSet};
use std::env;
use std::process::{Command, Stdio};

/// Format timestamp (milliseconds) to human-readable date
fn format_date(timestamp_ms: i64) -> String {
    let timestamp_sec = timestamp_ms / 1000;
    let datetime: DateTime<Utc> = Utc.timestamp_opt(timestamp_sec, 0).unwrap();
    let local: DateTime<Local> = datetime.with_timezone(&Local);
    local.format("%Y-%m-%d %H:%M").to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sessions_only = env::args().any(|arg| {
        matches!(
            arg.as_str(),
            "--sessions-only" | "--with-sessions" | "--non-empty"
        )
    });

    let runtime = ManagedRuntime::start_for_cwd().await?;
    println!(
        "Managed runtime ready at: http://localhost:{}",
        runtime.server().port()
    );
    let client = runtime.client();

    // List all projects from the project endpoint
    let projects = client.project().list().await?;

    // Extract unique project directories
    let project_dirs = unique_project_directories(&projects);

    if project_dirs.is_empty() {
        println!("\n\x1b[1;33m⚠️  No projects found\x1b[0m\n");
        return Ok(());
    }

    let base_url = runtime.server().url().to_string();
    let session_counts = fetch_session_counts(&base_url, &project_dirs).await?;

    let project_dirs = if sessions_only {
        project_dirs
            .into_iter()
            .filter(|dir| session_counts.get(dir).copied().unwrap_or(0) > 0)
            .collect::<Vec<_>>()
    } else {
        project_dirs
    };

    if project_dirs.is_empty() {
        if sessions_only {
            println!("\n\x1b[1;33m⚠️  No projects with sessions found\x1b[0m\n");
        } else {
            println!("\n\x1b[1;33m⚠️  No projects found\x1b[0m\n");
        }
        return Ok(());
    }

    // Check if fzf is available
    let fzf_available = Command::new("fzf").arg("--version").output().is_ok();

    if fzf_available {
        // Use fzf for interactive selection
        select_project_with_fzf(&base_url, &project_dirs, &session_counts).await?;
    } else {
        // List all projects without fzf
        list_all_projects(&project_dirs, &session_counts)?;
    }

    Ok(())
}

async fn fetch_sessions_for_directory(
    base_url: &str,
    directory: &str,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let url = format!("{}/session", base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .get(url)
        .header("x-opencode-directory", directory)
        .send()
        .await?
        .error_for_status()?;
    let sessions = response.json::<Vec<serde_json::Value>>().await?;
    Ok(filter_sessions_for_directory(sessions, directory))
}

fn filter_sessions_for_directory(
    sessions: Vec<serde_json::Value>,
    directory: &str,
) -> Vec<serde_json::Value> {
    let needle = directory.trim_end_matches('/');
    sessions
        .into_iter()
        .filter(|session| {
            let Some(dir) = session.get("directory").and_then(serde_json::Value::as_str) else {
                return false;
            };
            let dir = dir.trim_end_matches('/');
            dir == needle || dir.starts_with(&format!("{}/", needle))
        })
        .collect()
}

async fn fetch_session_counts(
    base_url: &str,
    project_dirs: &[String],
) -> Result<HashMap<String, usize>, Box<dyn std::error::Error>> {
    let mut counts = HashMap::new();

    for dir in project_dirs {
        let sessions = fetch_sessions_for_directory(base_url, dir).await?;
        counts.insert(dir.clone(), sessions.len());
    }

    Ok(counts)
}

fn project_directory(project: &Project) -> Option<String> {
    if let Some(dir) = &project.directory {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    project
        .extra
        .get("worktree")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn unique_project_directories(projects: &[Project]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut dirs = Vec::new();

    for project in projects {
        if let Some(dir) = project_directory(project) {
            if seen.insert(dir.clone()) {
                dirs.push(dir);
            }
        }
    }

    dirs
}

fn sorted_project_dirs_by_sessions(
    project_dirs: &[String],
    session_counts: &HashMap<String, usize>,
) -> Vec<String> {
    let mut sorted = project_dirs.to_vec();
    sorted.sort_by(|a, b| {
        let count_b = session_counts.get(b).copied().unwrap_or(0);
        let count_a = session_counts.get(a).copied().unwrap_or(0);
        count_b.cmp(&count_a).then_with(|| a.cmp(b))
    });
    sorted
}

async fn select_project_with_fzf(
    base_url: &str,
    project_dirs: &[String],
    session_counts: &HashMap<String, usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let sorted_dirs = sorted_project_dirs_by_sessions(project_dirs, session_counts);

    // Format projects for fzf: "directory (N sessions)"
    let project_list: Vec<String> = sorted_dirs
        .iter()
        .map(|dir| {
            let sessions_count = session_counts.get(dir).copied().unwrap_or(0);
            format!("{} ({} sessions)", dir, sessions_count)
        })
        .collect();

    // Create fzf input
    let fzf_input = project_list.join("\n");

    // Run fzf
    let mut child = Command::new("fzf")
        .arg("--prompt=Select project: ")
        .arg("--height=50%")
        .arg("--reverse")
        .arg("--header=Project Directory (Number of Sessions)")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    // Write project list to fzf stdin
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(fzf_input.as_bytes())?;
    }

    // Get output
    let output = child.wait_with_output()?;

    if !output.status.success() {
        println!("\n\x1b[1;33m⚠️  No project selected\x1b[0m\n");
        return Ok(());
    }

    // Parse selected project
    let selected = String::from_utf8_lossy(&output.stdout);
    let selected_dir = selected.split(" (").next().unwrap_or("").trim();

    if selected_dir.is_empty() {
        println!("\n\x1b[1;33m⚠️  Could not parse selection\x1b[0m\n");
        return Ok(());
    }

    // Display sessions for selected project
    let sessions = fetch_sessions_for_directory(base_url, selected_dir).await?;
    display_project_sessions(selected_dir, &sessions)?;

    Ok(())
}

fn display_project_sessions(
    project_dir: &str,
    sessions: &[serde_json::Value],
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "\n\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!("\x1b[1;33m📁 Project: {}\x1b[0m", project_dir);
    println!("\x1b[1;33m💬 Conversations: {}\x1b[0m", sessions.len());
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m\n"
    );

    // Sort sessions by creation time (oldest first, newest last)
    let mut sorted_sessions: Vec<&serde_json::Value> = sessions.iter().collect();
    sorted_sessions.sort_by(|a, b| {
        session_created_ms(a)
            .unwrap_or(0)
            .cmp(&session_created_ms(b).unwrap_or(0))
    });

    for (i, session) in sorted_sessions.iter().enumerate() {
        let id = session
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let title = session
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Untitled");
        println!("\x1b[1;35m  {}.\x1b[0m", i + 1);
        println!("     \x1b[33mID:\x1b[0m    {}", id);
        println!("     \x1b[33mTitle:\x1b[0m {}", title);
        if let Some(created) = session_created_ms(session) {
            let date_str = format_date(created);
            println!("     \x1b[33mCreated:\x1b[0m {}", date_str);
        }
        println!();
    }

    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m\n"
    );

    Ok(())
}

fn session_created_ms(session: &serde_json::Value) -> Option<i64> {
    session
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(serde_json::Value::as_i64)
}

fn list_all_projects(
    project_dirs: &[String],
    session_counts: &HashMap<String, usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "\n\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!(
        "\x1b[1;33m📁 Projects ({} total)\x1b[0m",
        project_dirs.len()
    );
    println!(
        "\x1b[1;36m═══════════════════════════════════════════════════════════════════════════════\x1b[0m\n"
    );

    // Sort projects by session count (desc), then directory (asc)
    let sorted_dirs = sorted_project_dirs_by_sessions(project_dirs, session_counts);

    for dir in sorted_dirs {
        let sessions_count = session_counts.get(dir.as_str()).copied().unwrap_or(0);
        println!("  \x1b[33m{}\x1b[0m ({} sessions)", dir, sessions_count);
    }

    println!();
    println!("\x1b[90m  Install fzf to enable interactive selection:\x1b[0m");
    println!("\x1b[90m    brew install fzf\x1b[0m");
    println!();

    Ok(())
}
