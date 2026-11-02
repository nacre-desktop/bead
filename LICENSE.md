# License boundaries

- Bead is an independent, general Wayland shell framework, licensed MIT only.
  `crates/bead-*` and the `crates/bead` facade are MIT, copyright (c) 2026
  siverteh, with permissive dependencies only. Nacre depends on Bead; Bead
  never depends on Nacre.
- `crates/nacre-shell` and `shell/` remain GPL-3.0, retaining Nacre's terms
  and notices until they move to the Nacre repository. They are the product,
  not part of the Bead framework; their move is planned in
  [B0214](docs/specs/0214-framework-split.md).
- `shell/welcome/demo/*.jpg`: CC0, derived from Nacre's bundled demo wallpapers;
  `palettes.json` records their artists, source URLs, source checksums and changes.
- Authored research prose/tooling/spikes/examples: MIT unless separately
  noted. Source facts/short extracts remain attributed to their original source.
  `spikes/reference/nacre-placement/` retains its explicit GPL-3.0-only product
  reference notices; the mirrored references in `ssh-placeholder/` do too.
- `spikes/blank-layer/LICENSE-SCTK` retains the MIT SCTK example notice.
- `vendor/fontconfig` and `vendor/system-tray` retain their upstream MIT notices.
- Test fonts retain their own terms: IBM Plex Sans is OFL-1.1; FixtureSymbols
  is Apache-2.0. Their full notices remain in `crates/bead-test/fonts/`.
  Dependency license records and historical evidence retain upstream terms.

Full first-party license texts: LICENSE-MIT and LICENSE-GPL-3.0.
Each future crate declares its own license and publish=false. A repository-wide
default must not accidentally relicense the GPL shell or third-party assets.
Installed icon/font assets retain their own licenses; assets require an audit
before being copied here. No GPL/LGPL implementation is copied into Bead crates.
