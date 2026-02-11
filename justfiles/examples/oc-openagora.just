# Run OpenAgora example
# Usage: just oc-openagora <prompt>
# Example: just oc-openagora "Explain quantum computing"

oc-openagora *ARGS:
    cargo run --example oc_openagora --features "full server" -- "{{ARGS}}"
