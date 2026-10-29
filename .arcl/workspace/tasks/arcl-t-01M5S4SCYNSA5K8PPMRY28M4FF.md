+++
id = "arcl-t-01M5S4SCYNSA5K8PPMRY28M4FF"
phase = "arcl-ph-01M5S4SCJ1A54Z514HCP508F9X"
title = "Build the web UI in GPUI"
status = "cancelled"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5S4SCNKRK07FF2PEBNVZKVX", "arcl-t-01M5ZR8QXTR076TPDJBXXNZPDG", "arcl-t-01M5ZR8QYWE6DXS33ZWC50DGQV"]
+++


PLAN.md's **Web UI** decision and [Pi design](../../../docs/pi-design.md#web-ui)
describe it.

Acceptance criteria:

- [ ] The UI shows runs, steps and logs, and the PR status Pi's status area shows
- [ ] It offers the same operations the `/storm:` comments trigger
- [ ] If the GPUI spike ships it in the browser, a Cloudflare Worker serves it behind GitHub sign-in
- [ ] If the spike picks a desktop client, the client reaches `fab serve` through the same Worker and sign-in

Cancelled: fabrial is the engine, not the host (PLAN.md). Hosting, other runner backends, artifact storage and web UIs are outside its scope.
