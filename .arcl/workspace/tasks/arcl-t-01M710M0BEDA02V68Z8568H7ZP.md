+++
id = "arcl-t-01M710M0BEDA02V68Z8568H7ZP"
parent = "arcl-t-01M710M04XDNTYD73VFN2VHGE3"
title = "Move the docs into the site by Diátaxis"
status = "pending"
priority = "high"
position = 0
blocked-by = ["arcl-t-01M710M06CV1KDKPF442PZZT3E", "arcl-t-01M710M0ADW67Y2X87ERTH97FK"]
+++

Move the content from README and `docs/` into the site, split by Diátaxis:

- Tutorials: set up a repository; take a first task from claim to merge.
- How-to guides: run the loop on Claude Code, Codex or Pi; set up gates;
  connect a tracker; resync a PR; update fabrial; opt in to the
  environment guard; turn on `signoff` or `assisted_by`; accept a head.
- Reference: the pages `fab manual` writes, plus the workflows, the PR
  commands and the installed files.
- Explanation: why fabrial is not a harness; the review loop and depth;
  the trust model; squash messages; claims as branches; the frozen Pi
  harness.

Acceptance criteria:

- [ ] Every fact in README and `docs/` lands on one page, and `docs/`
      keeps only what maintainers need (releases, the banner)
- [ ] README is short and points at the site; AGENTS.md's Layout matches
- [ ] Nothing names cloud, stormcloud or inkfinite
