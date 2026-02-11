# Publish to crates.io (bumps version, creates tag, publishes)
# Usage: just pub
# Example: just pub

pub:
    #!/usr/bin/env bash
    set -euo pipefail

    # Check for ANY uncommitted changes
    echo "🔍 Checking git status..."
    if ! git diff --quiet --exit-code || ! git diff --cached --quiet --exit-code; then
        echo "⚠️  Found uncommitted changes. Committing all changes before release..."
        git add .
        git commit -m "chore: prepare for release"
        echo "✓ Committed all pending changes"
    else
        echo "✅ Git working directory is clean"
    fi

    # Get current version from Cargo.toml
    CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    echo "Current version: $CURRENT_VERSION"

    # Parse version components
    MAJOR=$(echo $CURRENT_VERSION | cut -d. -f1)
    MINOR=$(echo $CURRENT_VERSION | cut -d. -f2)
    PATCH=$(echo $CURRENT_VERSION | cut -d. -f3)

    # Bump patch version
    NEW_PATCH=$((PATCH + 1))
    NEW_VERSION="${MAJOR}.${MINOR}.${NEW_PATCH}"
    echo "New version: $NEW_VERSION"

    # Update Cargo.toml (cross-platform sed)
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
    else
        # Linux
        sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
    fi
    echo "✓ Updated Cargo.toml to version $NEW_VERSION"

    # Update Cargo.lock by running cargo check
    echo "📝 Updating Cargo.lock..."
    cargo check --quiet 2>/dev/null || true
    echo "✓ Updated Cargo.lock"

    # Git commit the version bump
    echo "📝 Committing version bump..."
    git add Cargo.toml Cargo.lock 2>/dev/null || git add Cargo.toml
    git commit -m "chore: bump version to $NEW_VERSION"
    echo "✓ Committed version bump"

    # Create git tag
    echo "🏷️  Creating git tag v$NEW_VERSION..."
    git tag "v$NEW_VERSION"
    echo "✓ Created tag v$NEW_VERSION"

    # Push commits and tags (if remote exists)
    if git remote | grep -q .; then
        echo "🚀 Pushing to remote..."
        git push
        git push --tags
        echo "✓ Pushed commits and tags"
    else
        echo "⚠️  No git remote configured. Skipping push."
        echo "   To add a remote: git remote add origin <url>"
    fi

    # Publish to crates.io
    echo "📦 Publishing to crates.io..."
    cargo publish --allow-dirty
    echo "✓ Published to crates.io"

    echo ""
    echo "✅ Successfully published version $NEW_VERSION to crates.io"
