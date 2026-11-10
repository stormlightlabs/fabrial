---
title: Skills
description: The skills fab init installs, with what each does and the arguments it takes.
section: Reference
group: Skills
order: 8
---

`fab init` installs these skills into `.claude/skills/` and links
`.agents/skills` for Codex and Pi. Claude Code offers each as a slash command of
the same name, such as `/fab-implement`. A skill that takes arguments names them
after its description.

## fab-adv-rev

Adversarial review pass on a pull request or branch. Use for /fab-adv-rev, or
when asked for a worst-case or adversarial review.

Arguments: `[pr-number|branch]`.

## fab-decompose

Cut an idea, a spec or a review finding into tasks a run can take, grouped and
ordered, and write them to the repository's tracker. Use for /fab-decompose, or
when work is understood well enough to file but has not been filed.

Arguments: `<idea, spec or finding>`.

## fab-handoff

Record the state of unfinished work on its task, in the repository's tracker or
on its GitHub issue or pull request, so the next session can pick it up, or pick
up from the latest handoff. Use for /fab-handoff, when a session ends with work
left, or when a session starts by continuing earlier work.

Arguments: `[task|pick-up <task>]`.

## fab-implement

Work one task end to end on its own branch and push it so the bot opens a pull
request. The task is a tracked task, a GitHub issue, or a description. Use for
/fab-implement, when asked to start work on a task or issue, or when asked in
any words to build, fix, or change something in this repository.

Arguments: `<task-id|#issue|description>`.

## fab-resync

Bring an open pull request up to date with main by rebasing or merging, resolve
the conflicts, run the gates and push. Use for /fab-resync, or when a pull
request has a conflict or its base has moved under it.

Arguments: `<pr-number>`.

## fab-review

Review a pull request or branch for defects and report the findings. Runs a
standard pass or an adversarial pass. Use for /fab-review, /fab-adv-rev, code
review, PR review, or when asked to check a diff before merge.

Arguments: `[pr-number|branch] [standard|adversarial]`.

## fab-revise

Address the review findings on an open pull request and push the fixes. Use for
/fab-revise, or when asked to respond to review feedback on a pull request.

Arguments: `<pr-number>`.

## fab-rubber-duck

Work out a design with the person in rounds of questions, then record the
decisions as an idea, or as a spec when one decision must come before any work
can be cut. Use for /fab-rubber-duck, design discussion, or working out an
approach before any task exists.

Arguments: `[topic]`.

## fab-storm

Run one task (a tracked task, a GitHub issue or a description) or one open pull
request through implement and the review stages `fab sequence` gives, ending
with `Review finished at <sha>`. Use for /fab-storm, when asked to take a task
to a reviewed pull request, or to review and finish a pull request this session
opened.

Arguments: `<task|pr-number>`.

## fab-writing-docs

Write, revise, and deslop documentation for this repository in plain technical
English. Use for internal plans and specs, README, skills, error messages, and
any prose where AI writing tells, density, or readability matter.
