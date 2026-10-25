+++
id = "arcl-t-01M5S4SCR3B84DDGA6F8417RJB"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Queue messages in Pi while a task is active"
status = "pending"
priority = "normal"
position = 4
+++

Follow-up: a Pi extension that queues incoming questions and steering messages
while an agreed task is active.

Acceptance criteria:

- [ ] Queue incoming questions and steering messages while an agreed task is
      active
- [ ] Let the driver mark an interruption as urgent
- [ ] Otherwise answer at a safe boundary and continue the task without ending
      the run
- [ ] Show queued messages, and keep them through reload and resume
- [ ] Do not turn queued questions into new tasks or extra worker stages
