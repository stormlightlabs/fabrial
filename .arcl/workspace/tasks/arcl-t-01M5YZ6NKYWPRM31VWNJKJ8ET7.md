+++
id = "arcl-t-01M5YZ6NKYWPRM31VWNJKJ8ET7"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Tighten what review passes judge and post"
status = "pending"
priority = "normal"
position = 2
+++

From thunderstorm #143, #145 and #146.

Review passes judge the branch's commit messages, though the squash commit
takes the pull request's title and body. A dispatched pass cannot see its own
effort, so its opening line guesses it. Nothing checks a pass or an answer
against its word limit before it posts, and a bot review cannot be edited
after.

Acceptance criteria:

- [ ] The `review` skill and the reviewer and adversarial-reviewer agents
      judge the pull request title and body as the message that lands, and
      raise no finding about a branch commit's body
- [ ] The `storm` skill passes each dispatched agent the model and effort it
      dispatched with; a pass that was not told writes `not stated`
- [ ] `fab pr-review` and `fab pr-answer` refuse a body over its limit (200
      words for a pass, 150 for answers) and print the count, with tests at
      the limit and over it
