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

# arcl goes in a directory of its own, so putting it on PATH moves nothing
# else ahead of the image's tools.
bin="$HOME/.local/share/fabrial/arcl-$version/bin"
arcl=$bin/arcl
if [ "$("$arcl" --version 2>/dev/null)" != "arcl $version" ]; then
  mkdir -p "$bin"
  installer=$(mktemp)
  trap 'rm -f "$installer"' EXIT
  curl --proto '=https' --tlsv1.2 -fsSL -o "$installer" \
    "https://github.com/stormlightlabs/arclightning/releases/download/v$version/arcl-cli-installer.sh"
  ARCL_CLI_UNMANAGED_INSTALL="$bin" sh "$installer" --quiet
fi
path_line="export PATH=\"$bin:\$PATH\""
if [ "$(command -v arcl)" != "$arcl" ] && [ -n "${CLAUDE_ENV_FILE:-}" ] &&
  ! grep -qxF "$path_line" "$CLAUDE_ENV_FILE" 2>/dev/null; then
  echo "$path_line" >> "$CLAUDE_ENV_FILE"
fi

# The database is local and ignored by git. Build it once from the committed
# workspace; a resumed session keeps the one it has.
cd "$CLAUDE_PROJECT_DIR"
if [ ! -e .arcl/arcl.db ]; then
  "$arcl" init --quiet
fi
"$arcl" --version
