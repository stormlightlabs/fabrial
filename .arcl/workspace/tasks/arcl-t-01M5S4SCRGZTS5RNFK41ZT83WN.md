+++
id = "arcl-t-01M5S4SCRGZTS5RNFK41ZT83WN"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Host-check Git push and Rust gates from dispatch"
status = "pending"
priority = "normal"
position = 5
+++

Rust does not link `libiconv` inside the sandbox, so gates run on the host for
now. Earlier bootstrap builds and pushes did not go through Rust dispatch.

Acceptance criteria:

- [ ] Check Git push from a worker started by `fab dispatch`
- [ ] Check the Rust gates from a worker started by `fab dispatch`
