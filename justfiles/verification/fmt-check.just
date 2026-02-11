# Check code formatting
# Usage: just fmt-check
# Example: just fmt-check

fmt-check:
    @echo "🔍 Checking code formatting..."
    cargo fmt -- --check
    @echo "✅ Format check complete!"
