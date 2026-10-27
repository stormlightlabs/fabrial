+++
id = "arcl-t-01M5S4SCKKQSRQ76YJBHTF8NFG"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Owner: set up the bot and secrets across the org"
status = "pending"
priority = "normal"
position = 2
+++

Owner action. Install the bot and share its secrets with each repository that
uses fabrial.

Acceptance criteria:

- [ ] stormlight-thunderbot is installed on each repository
- [ ] The App ID and private key secrets are shared with each repository
- [ ] A ruleset requiring `storm/review` and the repository's check applies to
      a repository only after its `fab init` pull request merges, because the
      gate runs from the repository's `main`
- [ ] A tag ruleset protects `v*` tags, with the owner and the bot on its
      bypass list, so only the Release tag workflow creates one
      (thunderstorm #77)
