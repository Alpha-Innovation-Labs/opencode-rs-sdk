# Run a specific example
# Usage: just oc-example <name>
# Example: just oc-example basic

oc-example NAME *ARGS:
    cargo run --example {{NAME}} --features "http sse" -- "{{ARGS}}"
