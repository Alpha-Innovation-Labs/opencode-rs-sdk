# Interactively select and display an OpenCode conversation
# Usage: just oc-pick-conversation
# Displays full chat history with timestamps for the selected session
# Uses opencode-rs SDK to connect to server at http://127.0.0.1:4096

oc-pick-conversation:
    #!/usr/bin/env bash
    set -euo pipefail

    # Colors
    CYAN="\033[36m"
    GREEN="\033[32m"
    YELLOW="\033[33m"
    RED="\033[31m"
    RESET="\033[0m"

    # Check prerequisites
    if ! command -v fzf &> /dev/null; then
        echo -e "${RED}❌ Error: fzf is not installed${RESET}"
        echo "Install with: brew install fzf"
        exit 1
    fi

    echo -e "${CYAN}🔍 Fetching OpenCode sessions...${RESET}"

    # Run the Rust example to list sessions
    output=$(cargo run --example oc_list_sessions --features "http sse" 2>&1) || {
        echo -e "${RED}❌ Failed to fetch sessions${RESET}"
        if echo "$output" | grep -q "connection refused\|Connection refused"; then
            echo -e "${YELLOW}   Make sure the OpenCode server is running:${RESET}"
            echo -e "   opencode serve"
        else
            echo "$output"
        fi
        exit 1
    }

    # Extract just the JSON output (last line)
    SESSIONS=$(echo "$output" | tail -1)

    # Check if response is valid JSON array
    if ! echo "$SESSIONS" | jq -e 'type == "array"' >/dev/null 2>&1; then
        echo -e "${RED}❌ Invalid response from server:${RESET}"
        echo "$SESSIONS"
        exit 1
    fi

    # Check if sessions list is empty
    if [ "$SESSIONS" = "[]" ]; then
        echo -e "${YELLOW}⚠️  No OpenCode sessions found for current directory${RESET}"
        exit 0
    fi

    session_count=$(echo "$SESSIONS" | jq 'length')
    if [ "$session_count" -eq 0 ]; then
        echo -e "${YELLOW}⚠️  No sessions found${RESET}"
        exit 0
    fi

    # Format sessions for fzf (ID | Title | Last Activity)
    FORMATTED_SESSIONS=$(echo "$SESSIONS" | jq -r '.[] | "\(.id) | \(.title // "Untitled") | \(if .time then (.time.updated // .time.created | tostring) else "Unknown" end)"' 2>/dev/null) || {
        echo -e "${RED}❌ Failed to parse sessions${RESET}"
        exit 1
    }

    if [ -z "$FORMATTED_SESSIONS" ]; then
        echo -e "${YELLOW}⚠️  No sessions found${RESET}"
        exit 0
    fi

    # Use fzf for interactive selection
    SELECTED=$(echo "$FORMATTED_SESSIONS" | fzf \
        --prompt="Select conversation: " \
        --height=50% \
        --reverse \
        --header="ID | Title | Last Activity" \
        --preview="echo 'Press Enter to view conversation'" \
        --preview-window=down:1) || {
        echo -e "${YELLOW}⚠️  No session selected${RESET}"
        exit 0
    }

    if [ -z "$SELECTED" ]; then
        echo -e "${YELLOW}⚠️  No session selected${RESET}"
        exit 0
    fi

    # Extract session ID from selection
    SESSION_ID=$(echo "$SELECTED" | awk -F' | ' '{print $1}')

    echo -e "${CYAN}📖 Loading conversation: ${SESSION_ID}${RESET}"
    echo ""

    # Fetch and display full conversation using Rust example
    cargo run --example oc_get_conversation --features "http sse" -- "$SESSION_ID" 2>&1 || {
        echo -e "${RED}❌ Failed to fetch conversation${RESET}"
        exit 1
    }

    echo ""
    echo -e "${GREEN}✅ Conversation displayed${RESET}"
