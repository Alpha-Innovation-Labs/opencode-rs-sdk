# Clean (delete) conversations matching a pattern
# Usage: just oc-clean-conversations [pattern]
# Default pattern: "openagora" (case-insensitive)
# Shows preview first, then asks for confirmation before deleting

oc-clean-conversations *ARGS:
    cargo run --example oc_clean_conversations --features "http sse server" -- "{{ARGS}}" 2>&1
