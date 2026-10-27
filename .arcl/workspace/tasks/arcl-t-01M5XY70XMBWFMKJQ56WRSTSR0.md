+++
id = "arcl-t-01M5XY70XMBWFMKJQ56WRSTSR0"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Read the dispatch issuer credential through libsecret on Linux"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #62: on Linux the issuer reads its credential with `secret-tool`, smoke-tested against GNOME Keyring."
+++

`fab dispatch` reads the BWS machine token only from the macOS Keychain, so it cannot run on a Linux host. On Linux it reads the same entry from the Secret Service with libsecret's `secret-tool`, and the sandbox keeps workers away from the session bus.
