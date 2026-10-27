+++
id = "arcl-t-01M5YZ6NN465VCY6A493A6H4BY"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Answer the owner's review and cap open pull requests before new work"
status = "pending"
priority = "normal"
position = 5
+++

From thunderstorm #96 and #97.

A run should work in rounds: open pull requests up to a limit, let the owner
review them together, answer that review, then pick new work. Nothing counts
the bot's open pull requests, and nothing makes a run answer the owner's
review before it claims another task.

Acceptance criteria:

- [ ] `fab run` counts the bot's open pull requests in the repository, drafts
      included, and claims nothing at ten, saying the round is ready for review
- [ ] The owner's unanswered feedback on a pull request is each unresolved
      review thread whose latest comment is the owner's, and each conversation
      comment from the owner with no later bot reply
- [ ] `fab run` revises a pull request with unanswered owner feedback before
      it claims a new task
- [ ] Tests cover the limit, a thread the bot already answered, and an owner
      reply under a declined thread
