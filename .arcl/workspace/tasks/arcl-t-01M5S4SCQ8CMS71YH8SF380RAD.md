+++
id = "arcl-t-01M5S4SCQ8CMS71YH8SF380RAD"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Show the PR and CI state in the Pi status area"
status = "in_progress"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5S4SCPVN2EJB1X5YC9WXZG8"]
+++

Show the current pull request, its link and CI state in the fabrial Pi
package's status area. Follow [Pi design](../../../docs/pi-design.md#status-area).

Acceptance criteria:

- [ ] Show the PR link, its draft, ready or merged state and the current
      head's checks first, as the design's delivery order says
- [ ] Read PR and worker state from the shared collector instead of creating
      another status collector
- [ ] Use Pi's `ctx.ui.setStatus()` and `setWidget()`, keep Pi's context, usage
      and model data, and start no second terminal renderer
- [ ] Update when the shared state changes; mark stale data with its last
      successful refresh, and never show an earlier head's checks as the
      current head's
- [ ] Show a failed lookup as unknown, not green
- [ ] Links open in a browser, and the plain text still works when the
      terminal cannot open links
- [ ] At narrow widths, keep the PR, check state and current step and collapse
      the rest; use labels as well as color
- [ ] Show no step, budget or cost a source does not provide
- [ ] Dispose subscriptions on shutdown or reload; headless dispatch does not
      depend on the widgets or their timers
- [ ] Tests cover narrow terminals, missing PRs, failed checks, changed heads,
      rate limits and reloads
