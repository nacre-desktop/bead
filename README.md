# Bead

Bead is a general MIT-licensed Rust framework for building Wayland shells. It provides retained rendering and layout, Luau views, stable-ID state, reusable UI components, atomic hot reload, Wayland integration, and optional service adapters.

[Nacre](https://github.com/nacre-desktop/nacre) is one application built on Bead. The framework has no dependency on Nacre; application identity, content, assets, service routes, and installation policy belong to each host.

## Start here

[`examples/minimal-shell`](examples/minimal-shell/README.md) is a standalone host using only Bead. Its headless check is:

```sh
cargo run -p bead-minimal-shell --locked -- --check
```

Omit `--check` to open the Wayland window. The lower-level `bead --example minimal` is a renderer/facade smoke example.

## Framework map

- [`crates/`](crates/README.md) — framework crate guide.
- [`examples/`](examples/README.md) — minimal host and plugin example.
- [`docs/COMPONENTS.md`](docs/COMPONENTS.md) — UI components and host-provided theme inputs.
- [`docs/specs/`](docs/specs/README.md) — retained framework contracts and split decision.
- [`docs/development/ci.md`](docs/development/ci.md) — CI checks and local equivalents.

Views describe presentation and behavior in Luau. Rust owns reusable platform, rendering, input, and service primitives. Reload validates a complete new view generation before swapping it atomically; invalid changes leave the last working UI active and report a source location and error.

## CI and contributions

CI checks formatting, warnings-denied Clippy, tests, renderer features, framework dependency policy, and the minimal-shell example. See the [CI guide](docs/development/ci.md) and [repository rules](AGENTS.md) before contributing.

## License

Bead is distributed under MIT terms. Third-party components retain their own notices and licenses. See [LICENSE.md](LICENSE.md) and the adjacent notices under `vendor/` and `crates/bead-test/fonts/`.
