# Nacre protocol v1 fixtures

GPL-3.0-only; generated Nacre contract data retained in the GPL application.
The Rust contract crate is pinned in `crates/nacre-shell/Cargo.toml` to published
Nacre `70459b21c7010fb2422c7778c953b58ccf8011f0`. B0136 copies its
`wallpaper-active.jsonl` (manual-navigation readiness) and
`core-dashboard-updates.jsonl` (the published bounded package list).

Tests compare exact JSON wire objects with the pinned producer exports before
replay where applicable. Older compatibility captures are retained deliberately.
Object key order is not a wire contract; array order and fields remain checked.
These contracts do not imply runtime service availability. No protocol dependency
or fixture is embedded in a Bead framework crate.
