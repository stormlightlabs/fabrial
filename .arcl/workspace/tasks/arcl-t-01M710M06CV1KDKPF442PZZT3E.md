+++
id = "arcl-t-01M710M06CV1KDKPF442PZZT3E"
parent = "arcl-t-01M710M04XDNTYD73VFN2VHGE3"
title = "Generate reference pages with fab manual"
status = "completed"
priority = "high"
position = 0
evidence = "Done in #112 at 41cb20b: fab manual writes the CLI, settings and skills reference pages, checked for drift"
+++

Reference pages drift unless the code writes them. A hidden `fab manual
--out <dir>` walks the clap command tree and writes Markdown pages with
fanar frontmatter: one per command group, plus the `.github/fabrial.toml`
settings, the gates and sequence files, and one skills page with a
section per installed skill from its `SKILL.md` frontmatter.

Acceptance criteria:

- [x] `fab manual --out <dir>` writes the CLI, settings and skills pages
      with `title` and `description` frontmatter
- [x] A test fails when committed pages differ from what `fab manual`
      writes
- [x] The output follows the writing standard and needs no hand edits
