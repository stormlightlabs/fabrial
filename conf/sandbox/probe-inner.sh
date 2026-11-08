#!/usr/bin/env bash
# Runs inside the sandbox, in the worktree, in place of Pi. It tries each
# write a worker could use against the host and changes nothing else; probe.sh
# reads the result from the host side.
common=$(git rev-parse --path-format=absolute --git-common-dir)
gitdir=$(git rev-parse --path-format=absolute --git-dir)
branch=$(git symbolic-ref --short HEAD)
git config core.hooksPath /tmp/fab-probe-hooks
git config http.https://github.com.proxy http://fab-probe.invalid:8080
printf '#!/bin/sh\nexit 0\n' > "$common/hooks/pre-commit"
echo /etc > "$common/objects/info/alternates"
printf '[core]\n\tfsmonitor = /tmp/fab-probe-monitor\n' >> "$gitdir/config.worktree"
git update-ref refs/heads/fab-probe-other "$(git rev-parse HEAD)"
echo "$(git rev-parse HEAD) refs/heads/fab-probe-packed" >> "$common/packed-refs"
echo "probe $(date +%s)" > fab-probe.txt
git add fab-probe.txt && git -c user.name=probe -c user.email=probe@invalid commit -qm "fab probe"
git update-ref refs/heads/main HEAD
ln -s /etc/passwd "$common/refs/heads/fab-probe-link"
# These two point Git elsewhere, so they come last.
echo /tmp/fab-probe-common > "$gitdir/commondir"
echo "gitdir: /tmp/fab-probe-gitdir" > .git
true
