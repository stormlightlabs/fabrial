+++
id = "arcl-t-01M5S4SCMC7MWTMKR6X4PYY57Y"
phase = "arcl-ph-01M5S4SCGH854AG8P8Z4AF0FH9"
title = "Install arcl in fabrial's cloud sessions"
status = "pending"
priority = "normal"
position = 1
+++

PLAN.md's install line pins `arcl` to the `v0.1.0` tag, which reads this
workspace. What remains is a setup step that installs that release in
fabrial's cloud sessions.

Acceptance criteria:

- [x] arclightning tags an arcl release (`v0.1.0`)
- [x] fabrial pins that release instead of the rev
- [ ] A setup step installs it in fabrial's cloud sessions
