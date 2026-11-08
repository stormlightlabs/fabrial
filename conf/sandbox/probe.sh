#!/usr/bin/env bash
# Checks what a worker inside the sandbox can change in the Git state the host
# shares with it. It builds worker.nix with probe-inner.sh in place of Pi,
# runs it in a linked worktree of a scratch repository, and reads the
# repository from the host side. Run it from the repository root whenever the
# sandbox pin in worker.nix moves. It exits 1 when a write the host relies on
# being read-only took effect, or when a plain commit failed.
#
# It needs what dispatch needs: Nix, user namespaces, and /dev/net/tun usable
# by the user running it (pasta refuses root).
set -uo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# FAB_PROBE_WRAPPER names a wrapper already built with probe-inner.sh, for a
# user who cannot run nix-build.
wrapper=${FAB_PROBE_WRAPPER:-$(nix-build "$here/worker.nix" --arg entry "$here/probe-inner.sh" --no-out-link)} || exit 2
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch"/{input,runtime,state} && chmod 700 "$scratch/state"
git init -q -b main "$scratch/repo"
git -C "$scratch/repo" -c user.name=t -c user.email=t@invalid commit -q --allow-empty -m init
git -C "$scratch/repo" worktree add -q -b storm/probe "$scratch/wt"
start=$(git -C "$scratch/repo" rev-parse main)

(cd "$scratch/wt" && FAB_WORKTREE="$scratch/wt" FAB_INPUT="$scratch/input" \
  FAB_PI_RUNTIME="$scratch/runtime" FAB_STATE="$scratch/state" GH_TOKEN=probe GH_REPO=probe/probe \
  "$wrapper/bin/fab-pi-sandboxed" >"$scratch/log" 2>&1) || { cat "$scratch/log"; echo "fab probe: the sandbox did not run" >&2; exit 2; }

repo=$scratch/repo/.git
failed=0
held() { echo "held    $1"; }
leaked() { echo "LEAKED  $1"; failed=1; }
note() { echo "open    $1"; }

[ -z "$(git -C "$scratch/repo" config --get core.hooksPath)" ] && held "shared config: core.hooksPath" || leaked "shared config: core.hooksPath"
git -C "$scratch/repo" config --get-regexp 'http\..*\.proxy' >/dev/null && leaked "shared config: URL-scoped proxy" || held "shared config: URL-scoped proxy"
[ -e "$repo/hooks/pre-commit" ] && leaked "hooks/pre-commit" || held "hooks/pre-commit"
[ -e "$repo/objects/info/alternates" ] && leaked "objects/info/alternates" || held "objects/info/alternates"
[ "$(git -C "$scratch/repo" log -1 --format=%s storm/probe)" = "fab probe" ] && held "a plain commit lands on the worker's branch" || leaked "a plain commit lands on the worker's branch"
# The host's push reads refs as plain files and refuses symlinks, and Git
# reads config.worktree only with extensions.worktreeConfig, which sits in
# the read-only shared config. These are reported, not failed.
grep -q fsmonitor "$repo/worktrees/wt/config.worktree" 2>/dev/null && note "worktrees/<name>/config.worktree is writable" || held "worktrees/<name>/config.worktree"
[ -e "$repo/refs/heads/fab-probe-other" ] && note "other branch refs are writable" || held "other branch refs"
[ -L "$repo/refs/heads/fab-probe-link" ] && note "a branch ref can be a symlink" || held "branch ref symlinks"
[ "$(git -C "$scratch/repo" rev-parse main)" = "$start" ] || note "main moved"
exit $failed
