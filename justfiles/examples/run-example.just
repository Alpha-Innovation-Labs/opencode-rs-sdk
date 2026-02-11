# Run a specific example
# Usage: just example <name>
# Example: just example basic

example NAME *ARGS:
    cargo run --example {{NAME}} --features "http sse" -- "{{ARGS}}"
