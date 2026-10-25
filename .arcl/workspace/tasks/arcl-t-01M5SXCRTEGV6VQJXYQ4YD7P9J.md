+++
id = "arcl-t-01M5SXCRTEGV6VQJXYQ4YD7P9J"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Render shot.sh captures without color bleed"
status = "pending"
priority = "normal"
position = 0
+++

freeze 0.2.2 ignores SGR 39 and 49, which reset the foreground and background
to their defaults. In the PNGs `.github/storm/shot.sh` renders, a field's
color bleeds onto the separators and text that follow it. The `.ansi` capture
itself is correct.

Acceptance criteria:

- [ ] `.github/storm/shot.sh` normalizes the capture before rendering, for example by rewriting `ESC[39m` and `ESC[49m` into sequences freeze honours, so the PNG matches the terminal
- [ ] The saved `.ansi` stays as captured
- [ ] A capture of `printf '\e[35mA\e[39m B\n'` shows B in the default color
- [ ] `fab check` stays drift-free
