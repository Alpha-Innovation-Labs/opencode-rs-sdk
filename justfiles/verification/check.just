# Check code compiles without building
# Usage: just check
# Example: just check

check:
    @echo "🔍 Checking opencode-rs-sdk..."
    cargo check
    @echo "✅ Check complete!"
