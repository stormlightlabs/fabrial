+++
id = "arcl-t-01M6ZDKRD2VC50GCCKCG2896DS"
parent = "arcl-t-01M6ZBSFYBJPAAR89R51V6A1R4"
title = "Read the repository's App from fabrial.toml"
status = "in_progress"
priority = "high"
position = 0
+++

The first half of letting each repository use its own GitHub App: stop
naming stormlight-thunderbot anywhere fabrial installs, and rewrite the
README so a person new to fabrial can follow it.

Acceptance criteria:

- [ ] `.github/fabrial.toml` names the repository's App, and the gate, PR
      status, Branch PR and Bot comment read it instead of a fixed login
- [ ] The installed workflows read secret names that do not name a
      particular App, and `fab update` says how to move the old ones
- [ ] docs/bot-setup.md explains creating your own App; nothing installed
      into another repository names stormlight-thunderbot
- [ ] The README reads plainly for a newcomer: what fabrial is, how to
      install it, and one task end to end, with reference detail moved to
      docs/
