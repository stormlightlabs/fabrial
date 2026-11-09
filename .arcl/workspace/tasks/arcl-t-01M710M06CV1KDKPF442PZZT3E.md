+++
id = "arcl-t-01M710M06CV1KDKPF442PZZT3E"
parent = "arcl-t-01M710M04XDNTYD73VFN2VHGE3"
title = "Generate reference pages with fab manual"
status = "in_progress"
priority = "high"
position = 0
+++

Reference pages drift unless the code writes them. A hidden `fab manual
--out <dir>` walks the clap command tree and writes Markdown pages with
fanar frontmatter: one per command group, plus the `.github/fabrial.toml`
settings, the gates and sequence files, and one page per installed skill
from its `SKILL.md` frontmatter.

Acceptance criteria:

- [ ] `fab manual --out <dir>` writes the CLI, settings and skills pages
      with `title` and `description` frontmatter
- [ ] A test fails when committed pages differ from what `fab manual`
      writes
- [ ] The output follows the writing standard and needs no hand edits
