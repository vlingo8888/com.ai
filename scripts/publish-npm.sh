#!/usr/bin/env bash
set -e

# NPM Publish Shell Wrapper for @com.ai.vn/cli
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Ensure common paths are in PATH
export PATH="/usr/local/bin:/opt/homebrew/bin:$HOME/.bun/bin:$HOME/.cargo/bin:$PATH"

BUN_BIN="$(which bun 2>/dev/null || echo "$HOME/.bun/bin/bun")"

if [ ! -x "$BUN_BIN" ]; then
  echo "Error: bun runtime not found. Please install Bun from https://bun.sh"
  exit 1
fi

cd "$PROJECT_ROOT"
exec "$BUN_BIN" "$SCRIPT_DIR/publish-npm.ts" "$@"
