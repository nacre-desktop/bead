# Development preview

Use the standalone [`minimal-shell`](../examples/minimal-shell/README.md) to try Bead without an application checkout. Its headless check is:

```sh
cargo run -p bead-minimal-shell --locked -- --check
```

Omit `--check` to open a Wayland window. The example owns its views, palette, tokens, app ID, and fixtures. It does not require an application daemon or product configuration.

See [CI development](development/ci.md) for automated checks and [framework components](COMPONENTS.md) for the reusable UI kit and host inputs.
