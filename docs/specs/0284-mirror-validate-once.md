# 0284 Mirror validates once (B0284)

A host that mirrors a remote topic paid for each patch in the size of the whole topic, three
times over: `Mirror::patch` validated the merged object (limits: object root, 64 levels, 65,536
values, 8 MiB encoded), `Mirror::accept` validated it again, and `Publisher::publish_shared`
validated it a third time when the host republished it. `Publisher::change` also compared old
and new data twice.

## Contract

- Remote input is validated once, where it enters: `Mirror::accept` validates a snapshot,
  `Mirror::patch` validates the patch and the merged object. Both reject before the mirror changes.
- A `Mirror` only holds data that passed that check, so `Publisher::publish_mirror(&mirror)`
  republishes it without validating again (error `unavailable` when the mirror has no snapshot).
  `publish`, `publish_shared` and `publish_revision` still validate: their data can come from anywhere.
- `Publisher::change` compares old and new data once; no-op, revision and notification rules are unchanged.
- Hosts: replace `publisher.publish_shared(mirror.snapshot()?.data.clone())` after `accept`/`patch`
  with `publisher.publish_mirror(mirror)`. Without that change a patch validates twice, not three times.

Tests: `source::mirror_tests` (a `cfg(test)` counter shows one validation per patch plus
republish; oversized merged, too-deep and non-object input is rejected and leaves the mirror
unchanged; publication no-op and notification cases).

## Measurement

Per patch, a one-field patch on a topic of 400 / 1,300 rows (57 / 187 KB encoded), `Mirror::patch`
plus republish, release build, median of three runs of 400 patches (dev-2, ubuntu-bead image):

| Path | 57 KB: µs / allocs | 187 KB: µs / allocs |
|---|---|---|
| before, `publish_shared` | 332 / 5,700 | 1,156 / 18,450 |
| after, `publish_shared` (host unchanged) | 270 / 5,696 | 872 / 18,446 |
| after, `publish_mirror` | 220 / 5,692 | 787 / 18,442 |

One validation costs 53 / 175 µs and 4 allocations. The remaining cost is mostly the mirror's
copy of the old value before patching (113 / 371 µs, 5,672 / 18,422 allocations), which this
unit does not change.
