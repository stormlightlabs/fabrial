#!/bin/bash
# Prepares a Claude Code cloud session on fabrial: installs the pinned arcl
# release from its prebuilt installer, then builds the local arcl database
# from .arcl/workspace when the clone has none. Local sessions are left
# alone; install arcl there as PLAN.md's Work section says.
set -euo pipefail

# The arcl release this repository pins. PLAN.md's install line names the
# same version, and a test keeps the two equal.
version=0.3.0

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

bin=/usr/local/bin
if [ ! -w "$bin" ]; then
  bin="$HOME/.local/bin"
  mkdir -p "$bin"
fi

arcl=$bin/arcl
if [ "$("$arcl" --version 2>/dev/null)" != "arcl $version" ]; then
  installer=$(mktemp)
  trap 'rm -f "$installer"' EXIT
  curl --proto '=https' --tlsv1.2 -fsSL -o "$installer" \
    "https://github.com/stormlightlabs/arclightning/releases/download/v$version/arcl-cli-installer.sh"
  ARCL_CLI_UNMANAGED_INSTALL="$bin" sh "$installer" --quiet
fi
if [ "$(command -v arcl)" != "$arcl" ] && [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo "export PATH=\"$bin:\$PATH\"" >> "$CLAUDE_ENV_FILE"
fi

# The database is local and ignored by git. Build it once from the committed
# workspace; a resumed session keeps the one it has.
cd "$CLAUDE_PROJECT_DIR"
if [ ! -e .arcl/arcl.db ]; then
  "$arcl" init --quiet
  "$arcl" workspace import --quiet
fi
"$arcl" --version
