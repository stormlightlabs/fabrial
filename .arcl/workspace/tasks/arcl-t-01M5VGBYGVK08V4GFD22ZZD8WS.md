+++
id = "arcl-t-01M5VGBYGVK08V4GFD22ZZD8WS"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Split the worker runtime into its own crate"
status = "pending"
priority = "normal"
position = 0
+++

`fab dispatch`, the supervisor, the token issuer and the sandbox change at a different pace from the repo contract. Move them into their own crate with its own version, depended on by `fab`.
