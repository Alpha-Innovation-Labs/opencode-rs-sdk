# Run all tests
# Usage: just test
# Example: just test

test:
    @echo "🧪 Running all tests for opencode_rs..."
    cargo test --all-targets
    @echo "✅ All tests passed!"
