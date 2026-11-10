+++
id = "arcl-t-01M710M0C7Y6ZQ404T4DEFYVAB"
parent = "arcl-t-01M710M04XDNTYD73VFN2VHGE3"
title = "Deploy the docs site to Cloudflare"
status = "pending"
priority = "high"
position = 0
blocked-by = ["arcl-t-01M710M0ADW67Y2X87ERTH97FK"]
+++

Deploy the site to Cloudflare at fab.stormlightlabs.org on every push to
`main` that changes it.

Acceptance criteria:

- [ ] A workflow on `main` builds and deploys with wrangler
- [ ] The owner has added the Cloudflare token and the DNS record, and the
      site answers at fab.stormlightlabs.org
