# Framework crates

The workspace contains reusable framework crates. The `bead` facade presents the common host API; focused crates provide core state and view contracts, rendering, UI, Wayland integration, backend adapters, services, wallpaper process support, widgets, and test utilities.

All crates are unpublished. Each crate owns its dependency declarations and retains third-party notices for vendored code, fonts, and fixtures. Applications consume selected crates from a pinned Bead revision and provide their own identity, assets, content, configuration, and service integration.
