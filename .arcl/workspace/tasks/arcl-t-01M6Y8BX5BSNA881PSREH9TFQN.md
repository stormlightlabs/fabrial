+++
id = "arcl-t-01M6Y8BX5BSNA881PSREH9TFQN"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Probe that the sandbox keeps workers out of shared Git state"
status = "completed"
priority = "high"
position = 0
evidence = "Done on claude/sandbox-probe: probe.sh checks the sandbox's read-only Git paths and a plain commit, and lists the writable ones"
+++

From the review of #98. The host now pushes a writer's commits, but a few
host-side git commands in a worker's worktree (`status`, `add`, `commit`)
still read that repository's config, because they write to it. They are
safe only if the sandbox keeps a worker from writing the shared Git state.
That rests on agent-sandbox's `launcher/lib/git_state.py` and has not been
checked at runtime.

Acceptance criteria:

- [x] `conf/sandbox/probe.sh` builds the sandbox with a script in place of
      Pi, runs it in a linked worktree of a scratch repository, and checks
      from the host side: the shared `config` (`core.hooksPath`, a
      URL-scoped proxy), `hooks/` and `objects/info/alternates` must refuse
      a write, and a plain `git commit` must land
- [x] It exits 1 when one of those fails, says which, and lists the paths
      the worker can still write: other branch refs, ref symlinks,
      `packed-refs` and `config.worktree`
- [x] docs/pi.md says what the sandbox protects and to run it when the
      sandbox pin in `worker.nix` moves
- [x] It has been run against the current pin, with its output in the PR
