# Bead documentation

Bead is a framework for building Wayland shells. Folder apps use a manifest and Luau files; Rust is optional unless you are writing a host or embedding Bead.

## Start here

- [A bar in 50 lines](tutorial/bar.md) — build and run a desktop bar in about 15 minutes.
- [Build your first shell widget](tutorial/widget.md) — add a plugin widget to the tutorial bar.
- [Example gallery](../examples/README.md) — runnable examples and what each demonstrates.
- [Folder app manifest](reference/manifest.md) — every supported `bead.toml` key and validation error.
- [Luau UI reference](reference/luau-ui.md) — view elements, state, bindings, and actions.
- [Kit components](reference/kit.md) — reusable `bead.kit` components.
- [Services](reference/services.md) — desktop snapshots and declared command results/actions.
- [Rust hosts](reference/rust-hosts.md) — when to use Rust and the general folder host API.

## Architecture and contracts

- [Architecture decisions](decisions/) record framework-level design choices.
- [Specifications](specs/README.md) describe retained contracts, including the [general folder host](specs/0234-general-host.md) and [folder app services](specs/0235-folder-app-services.md).
- [Components overview](COMPONENTS.md) gives the shared kit and theme overview.
- [Contributing](../CONTRIBUTING.md) lists the build, the CI checks and the repository rules.

Bead is actively developed. Public APIs can change between reviewed revisions. The framework provides reusable platform and UI primitives; each host supplies its own identity, content, assets, service routes, configuration roots, and install policy.
