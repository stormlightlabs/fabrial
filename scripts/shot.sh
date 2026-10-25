#!/usr/bin/env bash
# Capture a terminal UI as a PNG and an ANSI text file.
# Usage: scripts/shot.sh <name> <cols> <rows> <command...>
set -euo pipefail

if [ "$#" -lt 4 ]; then
  echo "usage: just shot <name> <cols> <rows> <command...>" >&2
  exit 2
fi
name=$1 cols=$2 rows=$3
shift 3

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

session=fab-shot-$name
tmux new-session -d -s "$session" -x "$cols" -y "$rows"
trap 'tmux kill-session -t "=$session" 2>/dev/null || true' EXIT
tmux set-option -t "=$session:" remain-on-exit on
tmux respawn-pane -k -t "=$session:" -c "$PWD" -e "PATH=$PATH" "$@"

# Wait until the pane has drawn something and holds still for a second.
prev='' still=0
for _ in $(seq 20); do
  sleep 0.5
  cur=$(tmux capture-pane -p -e -t "=$session:")
  if [ -n "${cur//[[:space:]]/}" ] && [ "$cur" = "$prev" ]; then
    still=$((still + 1))
    [ "$still" -ge 2 ] && break
  else
    still=0
  fi
  prev=$cur
done

tmux capture-pane -p -e -t "=$session:" >"$out/$name.ansi"
freeze --language ansi -o "$out/$name.png" <"$out/$name.ansi" >/dev/null
echo "$out/$name.png"
