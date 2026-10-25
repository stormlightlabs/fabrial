+++
id = "arcl-c-01M5SWK1RV27M2KKX96RAJAB1Y"
title = "Flaky mailbox order test in just check"
status = "captured"
created-at = "2026-10-25T15:59:09.595Z"
+++

dispatch::bridge::tests::mailbox_preserves_submission_order_instead_of_uuid_order failed once in a full `just check` run on 2026-10-25 (branch storm/arcl-t-01M5S4SCQ8CMS71YH8SF380RAD, no Rust changes) and passed on three isolated reruns and the next full run. Likely timing-dependent under parallel tests.
