# Check code compiles without building
# Usage: just check
# Example: just check

check:
    @echo "🔍 Checking opencode_rs..."
    cargo check
    @echo "✅ Check complete!"
