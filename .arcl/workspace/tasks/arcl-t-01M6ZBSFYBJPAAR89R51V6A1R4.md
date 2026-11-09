+++
id = "arcl-t-01M6ZBSFYBJPAAR89R51V6A1R4"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Let each repository use its own GitHub App"
status = "completed"
priority = "high"
position = 0
evidence = "Done in #106 at a74cac1 and #108 at 29f2fa4: each repository names its own App, and the App key runs only from main"
+++

The first task after v0.9.0. The GitHub App that opens pull requests, posts
comments and sets `storm/review` belongs to the repository's user or
organization, not to stormlightlabs.

Today fabrial assumes stormlight-thunderbot: the installed workflows read
`THUNDERBOT_APP_ID` and `THUNDERBOT_PRIVATE_KEY`, the gate and PR status code
look for the `stormlight-thunderbot[bot]` login, and the docs and the section
`fab init` writes name it.

Acceptance criteria:

- [x] `.github/fabrial.toml` names the repository's App, and the gate, PR
      status, Branch PR and Bot comment read it instead of a fixed login
- [x] The installed workflows read secret names that do not name a
      particular App, and `fab update` says how to move the old ones
- [x] Workflows that hold the App key run only from `main`, which the child
      task covers
- [x] docs/bot-setup.md explains creating your own App; nothing installed
      into another repository names stormlight-thunderbot
