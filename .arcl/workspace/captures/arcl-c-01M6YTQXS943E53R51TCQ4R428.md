+++
id = "arcl-c-01M6YTQXS943E53R51TCQ4R428"
title = "Build a Pi harness on top of fabrial"
status = "captured"
created-at = "2026-11-09T00:18:46.185Z"
+++

An open source harness built on Pi or Tau, for a later release. It would
cover what fabrial deliberately leaves out: an MCP server, a worker runtime
that dispatches and sandboxes agents, and a small Pi setup that runs the
loop unattended.

These are unopinionated enough that each needs its own setup, so Pi should
drive their development, since it knows its own extension points best.
fabrial stays the contract, the skills, the workflows and the `fab`
commands; the harness would call them. The Pi harness code frozen in this
repository (dispatch, the sandbox, `fab run`, run events and the Pi
package) is a starting point, not a commitment.
