#!/bin/bash
# Post-tool-use hook: validates Rust code after Edit/Write operations.
# Runs cargo fmt --check and cargo clippy on changed files.

set -euo pipefail

# Only run for file modification tools
if [[ "${CLAUDE_TOOL_NAME:-}" != "Edit" && "${CLAUDE_TOOL_NAME:-}" != "Write" ]]; then
    exit 0
fi

# Get the file path from tool input
FILE_PATH="${CLAUDE_FILE_PATH:-}"
if [[ -z "$FILE_PATH" ]]; then
    exit 0
fi

# Only validate Rust and TOML files
case "$FILE_PATH" in
    *.rs|*.toml) ;;
    *) exit 0 ;;
esac

# Find workspace root (directory with Cargo.toml that has [workspace])
WORKSPACE_ROOT="$(cd "$(dirname "$FILE_PATH")" && cargo locate-project --workspace --message-format=plain 2>/dev/null | xargs dirname 2>/dev/null || true)"
if [[ -z "$WORKSPACE_ROOT" ]]; then
    exit 0
fi

cd "$WORKSPACE_ROOT"

# Run fmt check (fast, ~0.5s)
FMT_OUTPUT=$(cargo fmt --all --check 2>&1) || {
    echo "❌ cargo fmt check failed:"
    echo "$FMT_OUTPUT"
    echo ""
    echo "Run 'cargo fmt --all' to fix."
    exit 1
}

# Run clippy (slower, ~2-5s with cache)
CLIPPY_OUTPUT=$(cargo clippy --workspace -- -D warnings 2>&1) || {
    echo "❌ cargo clippy failed:"
    echo "$CLIPPY_OUTPUT"
    exit 1
}

echo "✅ fmt + clippy clean"
