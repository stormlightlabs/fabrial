+++
id = "arcl-t-01M5XWX8QYX093NVE5R4F4RFFH"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Publish fab to the stormlightlabs Homebrew tap"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #61: dist builds the `fab` formula and `.github/workflows/publish-homebrew.yml` commits it to stormlightlabs/homebrew-tap on each release."
+++

Each release commits a `fab` formula to stormlightlabs/homebrew-tap, so `brew install stormlightlabs/tap/fab` installs it. dist generates the formula; a publish job mints a thunderbot token for the tap instead of needing a personal token secret.
