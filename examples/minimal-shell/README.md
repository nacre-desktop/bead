# Minimal shell

An MIT example host with its own window identity, Luau views, palette, tokens,
input state and bundled hello plugin. It needs Bead and system Wayland/fonts;
it does not load a product runtime, helper/daemon, product fonts, or user configuration.
Theme tokens here are ordinary app Luau; framework kit token injection is a
separate follow-up.

```sh
nice -n 10 cargo run -p bead-minimal-shell --locked -j2 -- --check
nice -n 10 cargo run -p bead-minimal-shell --locked -j2
```

`--check` renders on the CPU without a Wayland connection, exercises an action
and text input, and checks state survives a source reload and the plugin renders.
The window uses app ID `org.bead.Example.MinimalShell`. Type a name, press the
counter button, then edit `ui/view/main.luau` to try validate-then-swap reload.
Invalid edits retain the previous scene and show a source diagnostic.

For development, start it inside a private nested compositor, using a private
HOME, XDG runtime directory and bus. `--seconds N` bounds the host's lifetime.
Use `--root DIR` to supply the bundled `ui/` tree when copying the binary elsewhere.
No installer, systemd unit or compositor config is installed by this example.
