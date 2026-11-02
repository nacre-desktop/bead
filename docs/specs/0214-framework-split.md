# B0214 — Independent MIT Bead framework

## Decision

Bead is a general MIT-licensed Wayland shell framework. Nacre is an application built on Bead and owns its product runtime, shell views, assets, packaging, product-specific tools, and product evidence. Framework crates remain application-neutral and do not depend on Nacre crates, paths, or commands.

The ownership inventory classified framework crates, the facade, vendor notices, minimal-shell, the neutral plugin example, and framework contracts as Bead-owned. Nacre-owned material included `crates/nacre-shell/`, `shell/`, product scripts, tools, spikes, packaging, product CI proofs, product docs, and product-only license files. Third-party notices remain beside retained framework code.

## Migration status

- Step 3: completed. Product history was imported into Nacre under `crates/nacre-shell/`, `shell/`, and `bead-shell/` paths.
- Step 4: implementation completed in this unit. Product paths, product-only checks, and product history were removed from Bead. Exact-commit CI verification is pending lead review; the framework workspace and minimal-shell must remain independent of a Nacre checkout.
- Step 5: closure checks remain. Confirm the pinned Nacre source and installer/bundle compatibility through the owning lead workflow; this does not change either repository's installation state.

Nacre's product specifications and historical evidence are maintained at [Nacre `bead-shell/docs`](https://github.com/nacre-desktop/nacre/tree/main/bead-shell/docs). Bead retains only reusable framework contracts listed in the [spec index](README.md).
