# 0285 UI package (B0285)

A long-running host that builds one view per output (a lock screen, say) re-read `ui/*.luau`
and `palette.json` for each view, so a package upgrade between two builds fed the new files to
an old binary. A `UiPackage` is a read-once snapshot; later views are built from memory.

## Contract

- `bead::ui::UiPackage::load(root, entry, palette) -> Arc<UiPackage>` runs one ordinary directory
  load (so it rejects what that rejects) and keeps the files it read: Luau modules, images, JSON,
  `bundle.json` and its shared files, plus the palette bytes. `entry` must be a `.luau` view.
- `FolderView::from_package(&pkg, title, size)` / `from_package_with_runtime(..)` (and
  `Session::from_package`) build a view with no filesystem access. Clone the `Arc` freely; each
  view has its own VM.
- Not captured: mounted plugins (`bundle.json` `plugins: true`; a package view refuses to build)
  and files outside the entry's directory, shared files and root images (`navigate` to another
  directory fails). Fonts and icon themes come from the host as before.
- Package views do not watch or reload from disk: `watch` is a no-op, `sources_current` and
  `check_sources` skip file comparison, a reload re-reads the same package.
- Recovery writes nothing unless the host calls `enable_recovery`, as for any session. For a
  package view that only records the last-good bundle after a presented frame; startup
  restore is skipped (the package is already the known-good copy).
- `Session::entry()` returns the entry as the host passed it, so a host whose root moves per
  deploy can key recovery by entry (and output) instead of `recovery_key`'s canonical root (B0400).
- `FolderView::load` is unchanged (it keeps live reload, so it is not a wrapper over the package).

Tests (`markup_tests`): edit then delete the directory, build another view from the package,
same text; package load rejects bad Luau and JSON entries; a missing `require` gives the same
error as a directory view.
