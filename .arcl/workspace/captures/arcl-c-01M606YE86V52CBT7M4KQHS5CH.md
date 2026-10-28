+++
id = "arcl-c-01M606YE86V52CBT7M4KQHS5CH"
title = "Open the pull request when work first lands on a claimed branch"
status = "discarded"
created-at = "2026-10-28T02:55:35.174Z"
+++

Branch PR runs only on the push that creates a branch. A claimed `storm/<task>` branch is created by `fab claim take` and holds only its claim then, so `fab branch-pr` opens nothing, and the later push that adds work skips the job. Today the implement skill falls back to `gh workflow run branch-pr.yml --ref <branch>`. A workflow condition, or a step that checks whether the previous tip was claim-only before building fab, could open it on that push.
