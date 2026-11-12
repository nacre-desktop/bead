# Rust hosts

Use Rust when you need a custom Wayland host, want to embed a Luau view in an existing application, or need to control surfaces and event mapping directly. A folder app using the `bead` CLI does not require Rust.

`bead::host::FolderView` loads a folder view, owns its UI `Session`, maps host events, and can CPU-render a check scene. The CLI combines it with `FolderServicesRuntime` and surface creation. This API is a reusable host seam; it does not encode app identity or installation policy.

Start with [`examples/minimal-shell`](../../examples/minimal-shell/README.md) for a standalone Rust host using Bead alone. See [spec 0234](../specs/0234-general-host.md) for the folder host contract and [spec 0235](../specs/0235-folder-app-services.md) for services.
