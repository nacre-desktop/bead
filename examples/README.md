# Examples

- [`hello-window`](hello-window/README.md) is a Luau-only app folder with an xdg-toplevel window, a button counter and a text input.
- [`hello-bar`](hello-bar/README.md) is a Luau-only top bar folder demonstrating edge placement and exclusive space reservation.
- [`minimal-shell`](minimal-shell/README.md) is a standalone Rust host demonstrating direct framework integration, reload and plugin use.
- [`plugins/hello`](plugins/hello/) is a neutral example plugin loaded by the minimal host.

The folder apps run with `bead run <folder>` and validate with `bead check <folder>`. The minimal host proof is `cargo run -p bead-minimal-shell --locked -- --check`.
