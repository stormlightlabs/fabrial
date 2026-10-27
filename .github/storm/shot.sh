#!/usr/bin/env bash
# Capture a terminal UI as a PNG and an ANSI text file. Needs tmux, freeze
# and perl.
# Usage: .github/storm/shot.sh [options] <name> <cols> <rows> <command...>
#
#   --wait-for <text>  wait until the screen shows <text>, then settle; fail
#                      if it does not appear before the timeout
#   --keys <key>       after the screen settles, send <key> with tmux
#                      send-keys, then settle again. Repeat it for more keys,
#                      sent in order: a tmux key name such as Enter or C-c is
#                      pressed, anything else is typed, as in
#                      --keys /reload --keys Enter
#   --timeout <s>      seconds to wait for the whole capture (default 10)
#
# The command runs on a tmux server of its own, so its options and its cleanup
# never touch the user's sessions. The script prints the PNG's path and exits
# with the command's status when the command has exited.
set -euo pipefail

usage() {
  echo "usage: .github/storm/shot.sh [--wait-for <text>] [--keys <key>]... [--timeout <s>] <name> <cols> <rows> <command...>" >&2
  exit 2
}

wait_for='' timeout=10 keys=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    --wait-for | --keys | --timeout) [ "$#" -ge 2 ] || usage ;;
  esac
  case "$1" in
    --wait-for) wait_for=$2 ;;
    --keys) keys+=("$2") ;;
    --timeout) timeout=$2 ;;
    --) shift; break ;;
    -*) usage ;;
    *) break ;;
  esac
  shift 2
done
[ "$#" -ge 4 ] || usage
case "$timeout" in
  '' | *[!0-9]*) echo "shot: --timeout takes whole seconds" >&2; exit 2 ;;
esac
name=$1 cols=$2 rows=$3
shift 3
case "$name" in
  '' | *[!A-Za-z0-9_-]*)
    echo "shot: name may hold only letters, digits, '-' and '_'" >&2
    exit 2
    ;;
esac

# Resolve the output directory through its nearest existing parent, so the
# check below runs before anything is created.
repo=$(cd "$(git rev-parse --show-toplevel)" && pwd -P)
ref=$(git symbolic-ref --short -q HEAD || git rev-parse --short HEAD)
out=${FAB_SHOT_DIR:-${TMPDIR:-/tmp}/fabrial-shots/${ref//\//-}}
case "$out" in /*) ;; *) out=$PWD/$out ;; esac
dir=${out%/} rest=''
while [ ! -d "$dir" ]; do
  rest=/${dir##*/}$rest
  dir=$(dirname "$dir")
done
out=$(cd "$dir" && pwd -P)$rest
case "$out/" in
  "$repo/"*)
    echo "shot: $out is inside the repository" >&2
    exit 2
    ;;
esac
mkdir -p "$out"

tmp=$(mktemp -d)
pane="=$name:"
t() { tmux -S "$tmp/tmux" -f /dev/null "$@"; }
trap 't kill-server 2>/dev/null || true; rm -rf "$tmp"' EXIT

# Start on a placeholder so options are set before the command runs. On a
# private server, the extended-keys server options reach only this capture.
# tmux before 3.5 has no extended-keys-format. remain-on-exit keeps the pane,
# its output and its exit status after the command exits, with no process
# left behind.
t new-session -d -s "$name" -x "$cols" -y "$rows" cat
t set-option -s extended-keys on
t set-option -s extended-keys-format csi-u 2>/dev/null || true
t set-option -g remain-on-exit on
t set-option -g remain-on-exit-format '' 2>/dev/null || true
t respawn-pane -k -t "$pane" -c "$PWD" -e "PATH=$PATH" "$@"

deadline=$((SECONDS + timeout))
dead() {
  local d
  d=$(t display-message -p -t "$pane" '#{pane_dead}') || {
    echo "shot: the tmux server stopped" >&2
    exit 1
  }
  [ "$d" = 1 ]
}

# Wait until the command exits, or the pane holds still for a second, or the
# timeout passes. Without an argument, a blank pane has not drawn yet.
settle() {
  local blank_ok=${1:-} prev='' cur still=0
  while [ "$SECONDS" -lt "$deadline" ]; do
    sleep 0.5
    dead && return
    cur=$(t capture-pane -p -e -t "$pane")
    if [ -n "$blank_ok${cur//[[:space:]]/}" ] && [ "$cur" = "$prev" ]; then
      still=$((still + 1))
      [ "$still" -ge 2 ] && return
    else
      still=0
    fi
    prev=$cur
  done
}

if [ -n "$wait_for" ]; then
  until [[ $(t capture-pane -p -J -t "$pane" 2>/dev/null) == *"$wait_for"* ]]; do
    if dead || [ "$SECONDS" -ge "$deadline" ]; then
      echo "shot: '$wait_for' did not appear within ${timeout}s" >&2
      exit 1
    fi
    sleep 0.2
  done
fi
if [ "${#keys[@]}" -gt 0 ]; then
  settle blank
  t send-keys -t "$pane" "${keys[@]}"
  settle blank
else
  settle
fi

t capture-pane -p -e -t "$pane" >"$out/$name.ansi"
# freeze 0.2.2 ignores the SGR codes that reset one attribute, such as 39
# and 49 for the default colors, so a color runs on past its field. Render
# from a copy in which each SGR sequence resets everything, then sets the
# attributes still on. The .ansi file keeps what tmux captured.
perl -pe '
  BEGIN { %on = () }
  sub sgr {
    my ($params) = @_;
    return "\e[${params}m" if $params =~ /:/;
    my @p = split /;/, $params, -1;
    @p = (0) unless @p;
    while (@p) {
      my $c = shift(@p) || 0;
      if ($c == 0) { %on = () }
      elsif ($c >= 1 && $c <= 9) { $on{$c} = $c }
      elsif ($c == 22) { delete @on{1, 2} }
      elsif ($c >= 23 && $c <= 29) { delete $on{$c - 20} }
      elsif (($c >= 30 && $c <= 37) || ($c >= 90 && $c <= 97)) { $on{fg} = $c }
      elsif ($c == 39) { delete $on{fg} }
      elsif (($c >= 40 && $c <= 47) || ($c >= 100 && $c <= 107)) { $on{bg} = $c }
      elsif ($c == 49) { delete $on{bg} }
      elsif ($c == 38 || $c == 48) {
        my $key = $c == 38 ? "fg" : "bg";
        my $mode = shift(@p) || 0;
        if ($mode == 5) { $on{$key} = join ";", $c, 5, shift(@p) || 0 }
        elsif ($mode == 2) { $on{$key} = join ";", $c, 2, map { shift(@p) || 0 } 1 .. 3 }
      }
    }
    return "\e[" . join(";", 0, grep { defined } @on{1 .. 9, "fg", "bg"}) . "m";
  }
  s/\e\[([0-9;:]*)m/sgr($1)/ge;
' "$out/$name.ansi" | freeze --language ansi -o "$out/$name.png" >/dev/null
echo "$out/$name.png"

if dead; then
  code=$(t display-message -p -t "$pane" '#{pane_dead_status}')
  if [ "${code:-0}" -ne 0 ]; then
    echo "shot: command exited with status $code" >&2
    exit "$code"
  fi
fi
