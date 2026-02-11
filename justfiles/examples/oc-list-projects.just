# List all projects (workspaces) and their conversations
# Usage: just oc-list-projects
# Interactive fuzzy search to select a project and view its conversations

oc-list-projects:
    cargo run --example oc_list_projects --features "http sse server" 2>&1
