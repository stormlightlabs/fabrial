#!/usr/bin/env bash
# Prints the squash commit body for a pull request: its description, then
# `Merged from <branch>.` and the owner's Co-authored-by line.
#
#   squash-body.sh <branch> <owner trailer> < description
#   squash-body.sh --text < description    the kept text alone
#
# The description stops at the footer line Branch PR writes, outside a code
# block, so the text after it stays out. The generated body from before the
# footer counts as empty. Lines that attribute the work to an agent are
# dropped, quoted or not and in any case: trailers (Co-authored-by,
# Claude-Session, Session), "Generated with" lines, the robot emoji, and Claude
# Code session and Codex task links. No agent attribution reaches main. A description with nothing left gives the plain
# `Merged from <branch>.` body. Branch PR and the Review gate both run this, so
# the two set the same message.
set -euo pipefail

text=$(tr -d '\r' | awk '
  /^[[:space:]]*(```|~~~)/ { fenced = !fenced }
  !fenced && /^<!-- fabrial:footer/ { footer = 1; exit }
  {
    # The line without quote marks and leading space, lowercased.
    bare = tolower($0); sub(/^[[:space:]>]*/, "", bare)
    if (bare ~ /^(co-authored-by|claude-session|session):/) next
    if (bare ~ /generated (with|by) .*(claude|codex)/) next
    if (bare ~ /claude\.ai\/code\/session_|chatgpt\.com\/codex\//) next
    if ($0 ~ /🤖/) next
  }
  { lines[++n] = $0 }
  END {
    first = 1; while (first <= n && lines[first] ~ /^[[:space:]]*$/) first++
    last = n; while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
    # The body Branch PR wrote before the footer existed is a list of commit
    # subjects under `From <branch>:`, which says nothing the title does not.
    if (!footer && lines[first] ~ /^From `[^`]+`:$/) exit
    for (i = first; i <= last; i++) print lines[i]
  }')

if [[ ${1:-} == --text ]]; then
  [[ -z $text ]] || printf '%s\n' "$text"
  exit 0
fi
: "${1:?branch}" "${2:?owner trailer}"
if [[ -n $text ]]; then
  printf '%s\n\n' "$text"
fi
printf 'Merged from %s.\n\nCo-authored-by: %s' "$1" "$2"
