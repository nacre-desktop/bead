# License boundaries

- `crates/bead-*`: MIT OR Apache-2.0, with permissive dependencies only.
- `crates/nacre-shell` and `shell/`: GPL-3.0, retaining Nacre's terms and notices.
- `shell/welcome/demo/*.jpg`: CC0, derived from Nacre's bundled demo wallpapers;
  `palettes.json` records their artists, source URLs, source checksums and changes.
- Authored research prose/tooling/spikes: MIT OR Apache-2.0 unless separately
  noted. Source facts/short extracts remain attributed to their original source;
  reference source implementations are not redistributed in this repository.
- `spikes/blank-layer/LICENSE-SCTK` retains the MIT SCTK example notice.

Full license texts: LICENSE-MIT, LICENSE-APACHE and LICENSE-GPL-3.0.
Each future crate declares its own license and publish=false. A repository-wide
default must not accidentally relicense the GPL shell or third-party assets.
Installed icon/font assets retain their own licenses; assets require an audit
before being copied here. No GPL/LGPL implementation is copied into Bead crates.
