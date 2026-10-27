+++
id = "arcl-t-01M5XSDPG74N3KNM6TFYF876PP"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Post review findings as inline comments on the diff"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in the pull request from `storm/arcl-t-01M5XSDPG74N3KNM6TFYF876PP`: `fab pr-review` posts a pass as one review with its findings on the diff, and `fab pr-answer` replies on the threads in one review and resolves the closed ones."
+++

Each review pass submits one bot review whose findings sit on the files and lines they name, so the owner gets one notification per pass and the reviser answers and resolves each thread. Findings on lines outside the diff go in the review summary. Agents post through a Bot comment workflow mode that takes a findings file, since they never hold the App key.
