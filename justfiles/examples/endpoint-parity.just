# Compare OpenAPI endpoints with locally implemented SDK endpoints
# Usage: just endpoint-parity <openapi-url> [scan-dir]
# Example: just endpoint-parity http://127.0.0.1:4096/doc
# Example: just endpoint-parity http://127.0.0.1:4096/doc src

endpoint-parity OPENAPI_URL SCAN_DIR='src':
    cargo run --example oc_endpoint_parity --features "http sse" -- \
      --crate-path "." \
      --openapi-url "{{OPENAPI_URL}}" \
      --scan-dir "{{SCAN_DIR}}"
