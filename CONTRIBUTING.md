# Contributing to Bead

Bead is MIT-licensed, and contributions are accepted under the same [license](LICENSE).

## Build and test

Install the toolchain from `rust-toolchain.toml` and the system packages from the first step of [CI](.github/workflows/ci.yml). The CI workflow is the authoritative framework check. Run focused checks while you work, then the workflow's steps before submitting; the core ones are:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 tools/check-framework-boundary.py
cargo run -p bead-minimal-shell --locked -- --check
cargo run -p bead-cli --locked -- check examples/hello-bar
```

CI also checks the boundary negative controls and service boundary, the renderer feature configurations, the `bead --example minimal` proof, every example folder app, private D-Bus service lifecycles, and dependency and license policy with `cargo deny` for each framework crate and the minimal shell. Checks must not require an application checkout: the workspace and `examples/minimal-shell` build and prove on their own.

## Rules

- Framework code is MIT. Add only dependencies whose licenses `deny-framework.toml` allows, and never copy GPL or LGPL code. Keep every third-party notice, including those under `vendor/` and `crates/bead-test/fonts/`.
- No application-specific code in framework crates: no application names, paths, service routes, assets or behavior. Hosts supply identity, content, configuration and policy. `tools/check-framework-boundary.py` enforces the dependency direction.
- Code is rustfmt-formatted and passes Clippy with warnings denied. Behavior changes come with tests.
- Every crate keeps `publish = false`.
- Committed files contain no personal paths, usernames or machine details.

## Specs and decisions

Framework contracts are specs in [`docs/specs/`](docs/specs/README.md): one file per contract, named `NNNN-name.md` and listed in the spec index. A change to a contract updates its spec in the same pull request; a new contract starts as a spec, and maintainers assign its number. Specs describe current behavior, stay application-neutral, and summarize evidence instead of committing raw logs or captures. Architecture decisions are recorded in [`docs/decisions/`](docs/decisions/).

Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).
