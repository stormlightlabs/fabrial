+++
id = "arcl-t-01M710M0ADW67Y2X87ERTH97FK"
parent = "arcl-t-01M710M04XDNTYD73VFN2VHGE3"
title = "Scaffold the docs site in packages/website"
status = "pending"
priority = "high"
position = 0
+++

Scaffold `packages/website/` with fanar: its own `package.json`, the root
`package.json` left as the Pi package's manifest. The theme overrides come
from docs/banner.png: near-black background, the gem blue as primary, pink
as the marker, green for success, a slab serif for headings and a
monospace for code, with a light mode that passes WCAG AA. The gem is the
logo, and the banner shows on the home page and the social card.

Acceptance criteria:

- [ ] The site builds with the published `@stormlightlabs/fanar`
- [ ] The Check workflow builds the site when it changes, and
      `.github/fabrial-gates.toml` has a scope for it
- [ ] Both themes pass the kit's contrast and axe checks
