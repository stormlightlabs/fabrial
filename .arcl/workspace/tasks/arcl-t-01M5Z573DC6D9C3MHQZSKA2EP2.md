+++
id = "arcl-t-01M5Z573DC6D9C3MHQZSKA2EP2"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Remove files a fabrial release no longer installs"
status = "pending"
priority = "normal"
position = 5
+++

Reported from arclightning's update to 0.7.1 (its PR 12).

`fab update` writes the files its release installs and leaves the rest alone,
so a file an older release installed and this one does not stays behind with
nothing calling it: `.github/storm/gate.sh` and `.github/storm/squash-body.sh`
after 0.7.1. `fab check` reads only the current list, so it reports nothing,
and README.md asks the owner to delete them by hand.

Acceptance criteria:

- [ ] fab-core keeps a list of the paths earlier releases installed and this
      one does not
- [ ] `fab check` reports each that exists
- [ ] `fab update` deletes each that still matches what a release wrote, and
      reports one that was edited instead of deleting it
- [ ] Tests cover a retired file deleted, an edited one kept and reported, and
      a repository without them; README.md drops its by-hand step
