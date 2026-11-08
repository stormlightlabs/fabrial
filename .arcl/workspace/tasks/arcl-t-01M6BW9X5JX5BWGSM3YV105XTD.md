+++
id = "arcl-t-01M6BW9X5JX5BWGSM3YV105XTD"
phase = "arcl-ph-01M5S4SCJ1A54Z514HCP508F9X"
title = "Offer the run operations as a library and a JSON event stream"
status = "completed"
priority = "high"
position = 0
blocked-by = ["arcl-t-01M5S4SCNKRK07FF2PEBNVZKVX"]
evidence = "Done in #100 at 10636b3: fab-core run registry and event stream with fab events"
+++

A host that serves the run operations over the network needs them as a
library and as a stream it can relay, without fabrial running a server.

Acceptance criteria:

- [x] fab-core's run operations are public, documented with rustdoc, and
      callable from another crate; a test calls them from outside fab-core
- [x] `fab events --follow` prints run events as JSON, one object per
      line, from dispatch's records under `state_dir`, as
      `fab worker events` reads them. PR status is not in the stream: hosts
      read it from GitHub, as the Pi package's `pr-status.ts` polls `gh`
- [x] `--after <cursor>` resumes the stream after the last event a host saw,
      and a test shows no event is repeated or skipped across a restart
- [x] Each line carries a format version, and docs/pi.md documents the fields
