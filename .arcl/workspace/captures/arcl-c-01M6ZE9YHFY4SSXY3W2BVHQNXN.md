+++
id = "arcl-c-01M6ZE9YHFY4SSXY3W2BVHQNXN"
title = "Take the Pi worker's git identity from the repository's App"
status = "captured"
created-at = "2026-11-09T06:00:39.727Z"
+++

conf/sandbox/worker.nix sets GIT_AUTHOR_NAME, GIT_AUTHOR_EMAIL, GIT_COMMITTER_NAME and GIT_COMMITTER_EMAIL to stormlight-thunderbot[bot] for every Pi worker, whatever repository it works in. fab init does not install the file, and the Pi harness is frozen, so the task that added `app` to .github/fabrial.toml left it alone. A repository with its own App that runs Pi workers would get commits under fabrial's bot identity. The wrapper could take the identity from the dispatching repository's `app`, or from the token's App.
