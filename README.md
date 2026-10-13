# Bead + Nacre Shell

Private feasibility experiment: a Rust Wayland shell framework with atomic hot
reload, and the Nacre desktop shell rebuilt on it. Not a replacement for the
installed Nacre shell.

Phase 0 researches the scope, dependencies, licensing and renderer costs. No
production framework or shell code is authorized until the owner approves
the recommendation. See [current status](docs/STATUS.md) and
[feasibility log](FEASIBILITY.md).

Planned licensing: `crates/bead-*` is MIT OR Apache-2.0. `crates/nacre-shell`
and `shell/` retain Nacre's GPL-3.0 terms. Each future crate must set
`publish = false`. Research tooling and measurement spikes are MIT OR Apache-2.0
unless an upstream notice specifies otherwise. License texts and Cargo workspace/CI cover research spikes now; production
framework/shell crates begin with Feature 1 after approval.

Never publish this repository, crates, packages or releases.
