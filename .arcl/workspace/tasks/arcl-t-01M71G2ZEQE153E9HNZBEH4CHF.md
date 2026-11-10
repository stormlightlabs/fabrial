+++
id = "arcl-t-01M71G2ZEQE153E9HNZBEH4CHF"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Point comment.yml at a page hosts can open"
status = "pending"
priority = "low"
position = 0
+++

Found while updating a host repository to 0.9.2. `.github/workflows/comment.yml`
cites `docs/bot-setup.md` near line 32, but `fab init` and `fab update` do
not install that file in a host repository.

Done when the installed workflow points at a page every host can open,
such as fabrial's docs site or its repository.
