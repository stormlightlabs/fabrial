+++
id = "arcl-t-01M5STSDQBSHD58XABCE599SFD"
spec = "arcl-s-01M5S4QY2GBQ3S3E6VA696T0K9"
plan = "arcl-pl-01M5S4SCF003F7F6Q4VQE44TFF"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Move the Pi package to packages/pi and the sandbox to conf/sandbox"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5S4SCQ8CMS71YH8SF380RAD"]
+++

Move the fabrial Pi package to `packages/pi/` as separate extensions, and the
Nix sandbox to `conf/sandbox/`.

Acceptance criteria:

- [ ] The Pi package lives under `packages/pi/`. It is split into separate
      extensions, each listed in the `pi.extensions` field of package.json:
      workers (the agent tool, agent_control and the /worker-* commands), PR
      status, and the status area. Queued messages get its own extension when
      that task lands. Shared code (journal, dispatch, the PR collector) sits
      in a lib folder that registers nothing with Pi. Extensions coordinate
      through Pi extension events (`fabrial:pr`, `fabrial:worker`) and never
      import each other's internals.
- [ ] `pi install <path>` and the `pi install git:…@v<version>` line in
      docs/pi.md still work. Check how Pi 1.4.0 resolves a package below the
      repository root before choosing where package.json lives.
- [ ] The Nix sandbox (`nix/worker.nix` and `deny-owner-keychain.patch`) moves
      to `conf/sandbox/`. The build command and paths in docs/pi.md are
      updated in the same PR, and the doc tells operators to rebuild and
      update `sandbox` in their dispatch config.
- [ ] `justfile`, `scripts/async-acceptance.mjs`, AGENTS.md Layout and every
      reference are updated. `just check` passes, and `fab check` reports no
      drift.
