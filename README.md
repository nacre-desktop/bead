# Bead

Bead is an actively developed, MIT-licensed Rust framework for building Wayland shells. It provides reusable rendering, input, Luau UI, widgets, host adapters, and optional desktop-service primitives. Bead is an independent framework; applications supply their own identity, shell content, assets, configuration, service routes, and installation policy.

**Status:** workspace crates are not published to crates.io. Applications should pin a reviewed Git revision; APIs may change between revisions.

## What Bead provides

Views describe presentation and behavior in Luau. Rust owns reusable platform, rendering, input, and service primitives. Reload validates a complete new view generation before swapping it atomically; invalid changes leave the last working UI active and report a source location and error.

## Try the minimal shell

[`examples/minimal-shell`](examples/minimal-shell/README.md) is a standalone host using only Bead. Its headless check is:

```sh
cargo run -p bead-minimal-shell --locked -- --check
```

Omit `--check` to open the Wayland window. The lower-level `bead --example minimal` is a renderer/facade smoke example.

## Getting started

Install or build the `bead` CLI, then check and run a folder app:

```sh
bead check examples/hello-bar
bead run examples/hello-bar
```

Start with the [documentation index](docs/README.md) and [15-minute bar tutorial](docs/tutorial/bar.md).

## Workspace layout

- [`crates/`](crates/) — framework crates.
- [`examples/`](examples/) — minimal host and plugin example.
- [`docs/`](docs/) — framework contracts and development guides.
- [`protocols/`](protocols/) — protocol definitions used by the workspace.
- [`tools/`](tools/) — repository checks and test support.

## Crates

See the [crate guide](crates/README.md) for the workspace crate breakdown.

## Examples

See the [examples guide](examples/README.md) for the minimal host and plugin example.

## Documentation

See the [documentation index](docs/README.md) for the tutorial, references, examples, architecture decisions, and specifications.

## License and third-party notices

Bead framework code and original documentation are licensed under the MIT License in [`LICENSE`](LICENSE). Third-party components retain their own notices and licenses; see the notices under `vendor/` and `crates/bead-test/fonts/`.

## Contributing

See the [CI guide](docs/development/ci.md) and [repository rules](AGENTS.md) before contributing.
