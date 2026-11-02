# ADR 0005 — Host-owned UI process model

Status: accepted as a framework boundary.

The framework provides UI sessions and window primitives; it does not prescribe how an application packages or launches them. A host may use one process or several according to its lifecycle and isolation needs. Each session receives explicit host configuration and releases its own view and rendering resources when retired.

Application-specific process names, command-line options, daemon links, and install/switch policy belong to the application repository.
