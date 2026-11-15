# Bead documentation

Bead is a framework for building Wayland shells. Folder apps use a manifest and Luau files; Rust is optional unless you are writing a host or embedding Bead.

## Start here

1. [Build the `bead` CLI](../README.md#get-started) and run `examples/hello-bar`.
2. [A bar in 50 lines](tutorial/bar.md): your first bar, with a live clock, battery and a mute button, in about 15 minutes.
3. [Build your first shell widget](tutorial/widget.md): a plugin widget mounted in that bar.
4. [Add a popup](tutorial/popup.md): a calendar panel opened from the bar's clock.
5. Panels that grow and join the bar: [`examples/bar-with-panels`](../examples/bar-with-panels/) and [Panel body and joined edges](reference/luau-ui.md#panel-body-and-joined-edges).
6. Reusable components: [`examples/kit-gallery`](../examples/kit-gallery/) and the [kit reference](reference/kit.md).
7. A Rust host, when a folder app is not enough: [Rust hosts](reference/rust-hosts.md) and [`examples/minimal-shell`](../examples/minimal-shell/README.md).

## Reference

- [Example gallery](../examples/README.md) — runnable examples and what each demonstrates.
- [Folder app manifest](reference/manifest.md) — every supported `bead.toml` key and validation error.
- [Luau UI reference](reference/luau-ui.md) — view elements, state, bindings, motion and actions.
- [Kit components](reference/kit.md) — reusable `bead.kit` components.
- [Services](reference/services.md) — desktop snapshots and declared command results/actions.
- [Rust hosts](reference/rust-hosts.md) — when to use Rust and the general folder host API.

## Architecture and contracts

- [Architecture decisions](decisions/) record framework-level design choices.
- [Specifications](specs/README.md) describe retained contracts, including the [general folder host](specs/0234-general-host.md) and [folder app services](specs/0235-folder-app-services.md).
- [Components overview](COMPONENTS.md) gives the shared kit and theme overview.
- [Contributing](../CONTRIBUTING.md) lists the build, the CI checks and the repository rules.

Bead is actively developed. Public APIs can change between reviewed revisions. The framework provides reusable platform and UI primitives; each host supplies its own identity, content, assets, service routes, configuration roots, and install policy.
