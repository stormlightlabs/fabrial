#!/usr/bin/env bash
# Capture a terminal UI as a PNG and an ANSI text file. Needs tmux and freeze.
# Usage: .github/storm/shot.sh <name> <cols> <rows> <command...>
#
# The command runs on a tmux server of its own, so its options and its cleanup
# never touch the user's sessions. The script prints the PNG's path and exits
# with the command's status when the command has exited.
set -euo pipefail

if [ "$#" -lt 4 ]; then
  echo "usage: .github/storm/shot.sh <name> <cols> <rows> <command...>" >&2
  exit 2
fi
name=$1 cols=$2 rows=$3
shift 3
case "$name" in
  '' | *[!A-Za-z0-9_-]*)
    echo "shot: name may hold only letters, digits, '-' and '_'" >&2
    exit 2
    ;;
esac

repo=$(git rev-parse --show-toplevel)
ref=$(git symbolic-ref --short -q HEAD || git rev-parse --short HEAD)
out=${FAB_SHOT_DIR:-${TMPDIR:-/tmp}/fabrial-shots/${ref//\//-}}
mkdir -p "$out"
out=$(cd "$out" && pwd -P)
case "$out/" in
  "$(cd "$repo" && pwd -P)/"*)
    echo "shot: $out is inside the repository" >&2
    exit 2
    ;;
esac

tmp=$(mktemp -d)
status=$tmp/status
pane="=$name:"
t() { tmux -S "$tmp/tmux" -f /dev/null "$@"; }
trap 't kill-server 2>/dev/null || true; rm -rf "$tmp"' EXIT

# Start on a placeholder so options are set before the command runs. On a
# private server, the extended-keys server options reach only this capture.
# tmux before 3.5 has no extended-keys-format.
t new-session -d -s "$name" -x "$cols" -y "$rows" sleep 86400
t set-option -s extended-keys on
t set-option -s extended-keys-format csi-u 2>/dev/null || true
# The wrapper records the exit status and keeps the pane open, so output from
# a command that exits at once stays on screen.
t respawn-pane -k -t "$pane" -c "$PWD" -e "PATH=$PATH" \
  sh -c 'f=$1; shift; "$@"; echo $? >"$f"; exec sleep 86400' sh "$status" "$@"

# Wait until the command exits, or the pane has drawn something and holds
# still for a second.
prev='' still=0
for _ in $(seq 20); do
  sleep 0.5
  [ -s "$status" ] && { sleep 0.2; break; }
  cur=$(t capture-pane -p -e -t "$pane")
  if [ -n "${cur//[[:space:]]/}" ] && [ "$cur" = "$prev" ]; then
    still=$((still + 1))
    [ "$still" -ge 2 ] && break
  else
    still=0
  fi
  prev=$cur
done

t capture-pane -p -e -t "$pane" >"$out/$name.ansi"
freeze --language ansi -o "$out/$name.png" <"$out/$name.ansi" >/dev/null
echo "$out/$name.png"

if [ -s "$status" ]; then
  code=$(cat "$status")
  if [ "$code" -ne 0 ]; then
    echo "shot: command exited with status $code" >&2
    exit "$code"
  fi
fi
