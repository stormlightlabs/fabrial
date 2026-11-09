+++
id = "arcl-t-01M6ZDKRD2VC50GCCKCG2896DS"
parent = "arcl-t-01M6ZBSFYBJPAAR89R51V6A1R4"
title = "Read the repository's App from fabrial.toml"
status = "completed"
priority = "high"
position = 0
evidence = "Done in #106 at a74cac1: fabrial.toml names the repository's App and secret prefix; README rewritten"
+++

The first half of letting each repository use its own GitHub App: stop
naming stormlight-thunderbot anywhere fabrial installs, and rewrite the
README so a person new to fabrial can follow it.

Acceptance criteria:

- [x] `.github/fabrial.toml` names the repository's App, and the gate, PR
      status, Branch PR and Bot comment read it instead of a fixed login
- [x] The installed workflows read `<PREFIX>_APP_ID` and
      `<PREFIX>_PRIVATE_KEY`, with the prefix from `secret_prefix`
      (`FABRIAL` by default), and `fab update` gives an older install
      `THUNDERBOT` so its secrets keep working
- [x] docs/bot-setup.md explains creating your own App; nothing installed
      into another repository names stormlight-thunderbot
- [x] The README reads plainly for a newcomer: what fabrial is, how to
      install it, and one task end to end, with reference detail moved to
      docs/
