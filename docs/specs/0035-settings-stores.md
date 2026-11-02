# Settings stores

Views address persisted values by stable IDs. Framework UI state and application settings remain separate: the framework defines binding and revision behavior while the host supplies storage locations and serialization policy.

A store update validates the candidate before replacement. Revision-aware writes let hosts reject stale updates, and opaque JSON values keep application schemas outside framework code. Failures preserve the previous valid state and return a diagnostic to the caller.
