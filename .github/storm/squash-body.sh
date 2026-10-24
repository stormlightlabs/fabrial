#!/usr/bin/env bash
# Prints the squash commit body for a pull request: its description, then
# `Merged from <branch>.` and the owner's Co-authored-by line.
#
#   squash-body.sh <branch> <owner trailer> < description
#   squash-body.sh --text < description    the kept text alone
#
# The description stops at the footer line Branch PR writes, so the text after
# it stays out. Co-authored-by and Claude-Session lines, Claude Code's
# "Generated with" line and session links are dropped, so no agent attribution
# reaches main. A description with nothing left gives the plain
# `Merged from <branch>.` body. Branch PR and the Review gate both run this, so
# the two set the same message.
set -euo pipefail

text=$(tr -d '\r' | awk '
  /^<!-- fabrial:footer/ { exit }
  tolower($0) ~ /^co-authored-by:/ { next }
  /^Claude-Session:/ { next }
  /Generated with \[Claude Code\]/ { next }
  /^https:\/\/claude\.ai\/code\/session_[A-Za-z0-9]+$/ { next }
  { lines[++n] = $0 }
  END {
    first = 1; while (first <= n && lines[first] ~ /^[[:space:]]*$/) first++
    last = n; while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
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
