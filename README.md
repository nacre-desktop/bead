# Bead + Nacre Shell

Feasibility experiment: a Rust Wayland shell framework with atomic hot
reload, and the Nacre desktop shell rebuilt on it. Not a replacement for the
installed Nacre shell.

ADR 0001 is approved. Feature 1 builds minimal surfaces on both compositors.
Each feature is measured and reported before the next starts. See [current status](docs/STATUS.md) and
[feasibility log](FEASIBILITY.md).

Planned licensing: `crates/bead-*` is MIT OR Apache-2.0. `crates/nacre-shell`
and `shell/` retain Nacre's GPL-3.0 terms. Each future crate must set
`publish = false`. Research tooling and measurement spikes are MIT OR Apache-2.0
unless an upstream notice specifies otherwise. License texts and Cargo workspace/CI cover research spikes now; production
framework/shell crates begin with Feature 1 after approval.

The repository is public by owner request. Do not publish crates/packages/releases.

Feature 1 development preview (from a Wayland terminal):

```sh
./scripts/dev.sh niri
```

A separate nested desktop opens with a dark 32-pixel strip. Close its window or
press Ctrl+C to stop. For the other compositor use `./scripts/dev.sh hyprland`.
Neither command replaces installed Nacre or changes session configuration.

CI checks fmt, clippy, tests and cargo-deny. Framework dependency graphs use
`deny-framework.toml`; the complete workspace, including GPL nacre-shell, uses
`deny.toml`. No new dependency/license ledgers are maintained.
