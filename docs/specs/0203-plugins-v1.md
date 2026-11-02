# Plugins v1 framework contract

A plugin is a directory containing a manifest and Luau entry files. The framework validates identifiers, file limits, API compatibility, entry paths, and declared capabilities before mounting it. Plugins may import the framework UI/kit and their own files; hosts decide which application helpers they expose.

The host supplies discovery roots, enablement policy, persistence paths, and any user-facing management UI. Framework capability checks apply to store reads and emitted actions. The Luau sandbox is a stability boundary, not a security boundary; hosts should enable only trusted plugins.

Malformed or faulting plugins are isolated and reported without replacing the last valid UI generation. Disabling a plugin removes it on the next mount. Product-specific widgets, routes, catalogs, and update policies remain host-owned.
