# Run clippy lints
# Usage: just clippy
# Example: just clippy

clippy:
    @echo "🔍 Running clippy on opencode-rs-sdk..."
    cargo clippy --all-targets -- -D warnings
    @echo "✅ Clippy complete!"
