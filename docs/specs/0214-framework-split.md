# B0214 — Independent MIT Bead framework

## Decision

Bead is a general MIT-licensed Wayland shell framework. Applications built on Bead own their runtime, shell views, assets, packaging, tools, and application-specific evidence. Framework crates remain application-neutral and do not depend on product crates, paths, or commands.

The ownership inventory classified framework crates, the facade, vendor notices, minimal-shell, the neutral plugin example, and framework contracts as framework-owned. Application-owned code and assets were removed without changing framework behavior. Third-party notices remain beside retained framework code.

## Retained framework surface

Bead retains rewritten framework guides and contracts, MIT license text, upstream font/vendor notices, `crates/bead*`, `vendor/`, `examples/minimal-shell`, `examples/plugins/hello`, and neutral tooling. Application runtime code, assets, and product-specific checks belong in their owning application repository.

## Migration status

- Framework and application ownership are separated.
- Framework CI and the independent minimal-shell proof run without an application checkout.
- The dependency guard rejects product crates, paths, and command references in the framework tree.
- Any remaining closure or compatibility work belongs to the owning application workflow and does not change this repository's installation state.

Bead retains reusable framework contracts listed in the [spec index](README.md).
