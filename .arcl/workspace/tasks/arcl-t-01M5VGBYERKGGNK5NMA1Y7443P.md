+++
id = "arcl-t-01M5VGBYERKGGNK5NMA1Y7443P"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Scale review depth to the pull request's risk"
status = "pending"
priority = "normal"
position = 0
+++

Docs-only, test-only and release PRs take one review pass or none; gate, release, workflow and security changes keep both. `fab` classifies a PR from its diff, as Check does for docs, and the storm skill reads the result.
