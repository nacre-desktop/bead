# ADR 0004 — Independent framework and application ownership

Status: accepted.

Bead is the MIT framework repository. Applications built on Bead own their runtime, views, assets, catalogs, service routes, packaging, and application-specific dependencies. Framework crates expose neutral APIs and depend only on permissively licensed libraries.

The framework/application split keeps application history and behavior out of Bead. Bead retains its own permissive dependency policy and required upstream notices. No GPL/LGPL implementation code is copied into framework crates.

See [B0214](../specs/0214-framework-split.md) for the framework ownership decision.
