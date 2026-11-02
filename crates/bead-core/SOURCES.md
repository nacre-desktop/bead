# Source and adapter notes

`bead::source` provides framework-level in-process adapters for notifications and desktop services. Applications choose transports, identity, catalogs, and routes at their boundary. The framework does not define an application daemon protocol or require a particular service host.

`bead-test::source::FakeSource` drives deterministic headless UI and timeout tests. Wire framing, peer authentication, and automatic socket reconnect are transport concerns for the host adapter and are not provided by these in-process test sources.
