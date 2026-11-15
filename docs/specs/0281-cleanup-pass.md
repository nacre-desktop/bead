# 0281 — Cleanup and bloat pass

Status: done. A net-negative pass after the [0238](0238-api-stability.md) cleanup units (B0256-B0258), checked against `main` at `841de925`. No runtime behaviour change. Net: product −230 lines (Rust −241, manifests and lock +11), specs −36 before this file.

## B0281 pass

| Item | Net lines |
|---|---:|
| One `bus::Snapshots<T>` (latest snapshot, subscribers, revision bump) for the media, power-profiles, Bluetooth and network services; each keeps its `Subscription` name as an alias | −164 |
| One `call` helper for the two identical notification `call` methods | −6 |
| One `App::shm_format` for the window, canvas and overlay draw paths (pixel and buffer format) | −37 |
| Unused probes: `BEAD_PRIVATE_MAIN_LOOP_STALL_MS` (watchdog stall), `BEAD_PROFILE_DAMAGE` (`INPUT_REPAIR` line), `BEAD_PRIVATE_CLIPBOARD_CORE` (forced core clipboard); no test, script or host sets them | −34 |
| Unused `accesskit` dependency of `examples/minimal-shell` | −2 |
| Specs: 0262's finished host-migration table, a duplicate 0271 index row | −36 |
| Dev profile: `opt-level = 3` for the image codecs (`image`, `png`, `fdeflate`, `miniz_oxide`, `crc32fast`, `adler2`, `tiny-skia`, `zune-*`) | +13 |

Already gone before this pass (verified): `source::Desktop`, `HostedNotifications`, `Notifications::private`, `tests/source.rs`, the duplicate `system_connection`, `spawn_worker`, the shared service `wait_for`, `Scene::empty`, `#[cfg(test)]` `i18n::resident_bytes`, `Mirror::needs_snapshot`.

Sweep since the audit (`9b065d1a..841de925`): no new dependencies (a source scan of every manifest found only the `accesskit` entry above); no stale `#[allow(dead_code)]` (the remaining ones are in tests or `cfg_attr` for the non-GL build); no new env gates; byte-identical function bodies only in the items above and the deferred ones below; every `pub` item has a Bead or host reference.

Deferred (owning unit):

- `host.rs` reads `media::monotonic_millis`; it can use `bead_core::profile::monotonic_ns() / 1_000_000` and media's clock can go private (B0272, host.rs). bead-services cannot depend on bead-core (service boundary).
- `runtime.rs` `Desktop` repeats the callback/`Subscription` pattern that `bus::Snapshots` now holds (B0275, clock files).
- `tree.rs` `intersection` duplicates `damage.rs` `intersect` (B0280, tree.rs).
- The slow `tree::tests` image fixtures stay: two are sized to cross the 40 MiB image budget (25 cards of 512×432, 24 cards of 690×570 at scale 1.5) and the third needs a source larger than its target. Their cost was debug-built codecs, hence the dev profile row: the three tests take 37.7 s → 8.9 s and the whole bead-render test binary 46.8 s → 10.0 s (dev-2, `-j2`), for about 30 s more cold build. Shrinking only `uhd_cover…` would need a `tree.rs` hunk (B0280).
