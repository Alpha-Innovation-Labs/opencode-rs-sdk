# Run unit tests only
# Usage: just test-lib
# Example: just test-lib

test-lib:
    @echo "🧪 Running unit tests for opencode-rs-sdk..."
    cargo test --lib
    @echo "✅ Unit tests passed!"
