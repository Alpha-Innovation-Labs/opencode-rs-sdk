# Compare OpenAPI endpoints with locally implemented SDK endpoints
# Usage: just oc-endpoint-parity <openapi-url> [scan-dir]
# Example: just oc-endpoint-parity http://127.0.0.1:4096/doc
# Example: just oc-endpoint-parity http://127.0.0.1:4096/doc src

oc-endpoint-parity OPENAPI_URL SCAN_DIR='src':
    cargo run --example oc_endpoint_parity --features "http sse" -- \
      --crate-path "." \
      --openapi-url "{{OPENAPI_URL}}" \
      --scan-dir "{{SCAN_DIR}}"
