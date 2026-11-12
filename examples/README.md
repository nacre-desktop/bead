# Example gallery

Run commands from the repository root after building or installing `bead`. Folder apps can be checked without a compositor; running them needs a Wayland session. Screenshots are included only when the folder has a small checked-in capture.

| Example | Shows | Run/check | Screenshot |
|---|---|---|---|
| [hello-window](hello-window/) | Basic window, text input, behavior action, computed counter. | `bead check examples/hello-window`; `bead run examples/hello-window` | — |
| [hello-bar](hello-bar/) | Minimal layer-shell bar with a title and fixed-width clock label. | `bead check examples/hello-bar`; `bead run examples/hello-bar` | — |
| [hello-status](hello-status/) | Desktop clock, battery/audio status and a declared `uname` command. | `bead check examples/hello-status`; `bead run examples/hello-status` | — |
| [tutorial-bar](tutorial-bar/) | Tutorial bar with centered clock, battery, volume and mute action. | `bead check examples/tutorial-bar`; `bead run examples/tutorial-bar` | — |
| [minimal-shell](minimal-shell/) | Standalone Rust host, stateful Luau window, plugin, reload and CPU check. | `cargo run -p bead-minimal-shell --locked -- --check`; `cargo run -p bead-minimal-shell --locked` | — |
| [plugins](plugins/) | Example third-party widget plugin manifest and Luau entrypoint. | Used by the minimal-shell plugin demonstration; see its README. | — |
