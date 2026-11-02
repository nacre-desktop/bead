# B0214 — Independent MIT Bead framework

## Decision

Bead is a general MIT-licensed Wayland shell framework. Nacre is an application built on Bead and owns its product runtime, shell views, assets, packaging, product-specific tools, and product evidence. Framework crates remain application-neutral and do not depend on Nacre crates, paths, or commands.

The ownership inventory classified framework crates, the facade, vendor notices, minimal-shell, the neutral plugin example, and framework contracts as Bead-owned. Nacre-owned material included `crates/nacre-shell/`, `shell/`, product scripts, tools, spikes, packaging, product CI proofs, product docs, and product-only license files. Third-party notices remain beside retained framework code.

## Frozen export

Nacre accepted step 3 at `8c56db8960b63b6d4cfabd98e2c281c85c03e68c`. Import commit `93444c488a332af174e853d206200f358d045911` records the path list, original tree IDs and history map from Bead `f19eacaf7b3dedfbb54202358d88e699444366b1`. Nacre continues to pin that exact Bead revision; this cleanup does not update it.

| Exported product selection | Nacre destination |
| --- | --- |
| `crates/nacre-shell/`, `shell/` | Same paths |
| `packaging/`, `scripts/`, `spikes/`, `ssh-placeholder/` | Under `bead-shell/` |
| `tools/daily-acceptance/`, `bake-welcome-demo.py`, `i18n-sync.py`, `check-luau.py` | Under `bead-shell/tools/` |
| Historical root example images/configs and guide | Under `bead-shell/examples/` |
| Historical docs, feasibility, README and notices | Under `bead-shell/` |
| Product `deny.toml` | `bead-shell/deny-product.toml` |

Bead retains rewritten framework guides/contracts, MIT text, upstream font/vendor notices, `crates/bead*`, `vendor/`, `examples/minimal-shell`, `examples/plugins/hello`, and neutral tooling. Product runtime code and assets are removed without changing framework behavior.

## Migration status

- Step 3: completed. Product history was imported into Nacre under `crates/nacre-shell/`, `shell/`, and `bead-shell/` paths.
- Step 4: implementation completed in this unit. Product paths, product-only checks, and product history were removed from Bead. Exact-commit CI verification is pending lead review; the framework workspace and minimal-shell must remain independent of a Nacre checkout.
- Step 5: closure checks remain. Confirm the pinned Nacre source and installer/bundle compatibility through the owning lead workflow; this does not change either repository's installation state.

Nacre's product specifications and historical evidence are maintained at [Nacre `bead-shell/docs`](https://github.com/nacre-desktop/nacre/tree/main/bead-shell/docs). Bead retains only reusable framework contracts listed in the [spec index](README.md).
