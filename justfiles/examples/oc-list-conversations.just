# List all OpenCode sessions from the local server
# Usage: just oc-list-conversations

oc-list-conversations:
    #!/usr/bin/env bash
    set -euo pipefail

    CYAN="\033[36m"
    GREEN="\033[32m"
    YELLOW="\033[33m"
    RED="\033[31m"
    BOLD="\033[1m"
    RESET="\033[0m"

    echo -e "${CYAN}🔍 Fetching OpenCode sessions...${RESET}"

    tmp_output=$(mktemp)

    # Run the Rust example
    cargo run --example oc_list_sessions --features "http sse server" >"$tmp_output" 2>&1 || {
        echo -e "${RED}❌ Failed to fetch sessions${RESET}"
        if grep -qi "connection refused" "$tmp_output"; then
            echo -e "${YELLOW}   Make sure the OpenCode server is running:${RESET}"
            echo -e "   opencode serve"
        else
            cat "$tmp_output"
        fi
        rm -f "$tmp_output"
        exit 1
    }

    # Extract just the JSON output (last line)
    response=$(tail -1 "$tmp_output")
    rm -f "$tmp_output"

    if [ -z "$response" ] || [ "$response" = "[]" ]; then
        echo -e "${YELLOW}⚠️  No sessions found${RESET}"
        exit 0
    fi

    if command -v jq &> /dev/null; then
        # Check if response is valid JSON array
        if ! echo "$response" | jq -e 'type == "array"' >/dev/null 2>&1; then
            echo -e "${RED}❌ Invalid response:${RESET}"
            echo "$response"
            exit 1
        fi
        
        session_count=$(echo "$response" | jq 'length')
        
        if [ "$session_count" -eq 0 ]; then
            echo -e "${YELLOW}⚠️  No sessions found${RESET}"
            exit 0
        fi

        echo -e "${BOLD}${CYAN}┌──────────────────────────────────────────────────────────────────────────┐${RESET}"
        echo -e "${BOLD}${CYAN}│${RESET} ${BOLD}OpenCode Sessions${RESET}                                                        ${BOLD}${CYAN}│${RESET}"
        echo -e "${BOLD}${CYAN}├──────────────────────────────────────────────────────────────────────────┤${RESET}"
        
        echo "$response" | jq -r '.[] | 
            "\n\u001b[1;36m│\u001b[0m" +
            "\n\u001b[1;36m│\u001b[0m \u001b[33mSession ID:\u001b[0m    " + .id +
            "\n\u001b[1;36m│\u001b[0m \u001b[33mTitle:\u001b[0m         " + (.title // "Untitled") +
            "\n\u001b[1;36m│\u001b[0m \u001b[33mDirectory:\u001b[0m     " + (.directory // "Unknown") +
            "\n\u001b[1;36m│\u001b[0m \u001b[33mCreated:\u001b[0m       " + (if .time then (.time.created | tostring) else "Unknown" end)
        '
        
        echo -e "${BOLD}${CYAN}└──────────────────────────────────────────────────────────────────────────┘${RESET}"
        echo ""
        echo -e "${GREEN}✅ Found ${session_count} session(s)${RESET}"
    else
        echo "$response"
    fi
