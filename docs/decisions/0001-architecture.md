# ADR 0001 — Framework architecture

Status: accepted.

Bead is a general MIT-licensed framework for Wayland shells. Rust owns platform integration, rendering, input, reusable UI and service primitives. Luau describes view content and behavior. Applications supply identity, content, assets, policy, and any application-specific service adapters.

Framework crates use permissive dependencies and remain independent of consuming applications. Product-only integrations and licensing stay in the product repository. Rendering backends remain behind framework interfaces; the default build must remain usable without requiring an application checkout.

This decision is implemented by the workspace crate boundaries and the standalone `examples/minimal-shell` host. The repository split and ownership record are in [B0214](../specs/0214-framework-split.md).
