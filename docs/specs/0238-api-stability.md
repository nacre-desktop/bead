# 0238 — API stability and versioning plan

Status: proposal for owner approval before any crates.io release. No crate is published today.

## 1. Surfaces and users

| Surface | Users | Classification | Contract boundary |
|---|---|---|---|
| `bead.toml`: identity, view/palette paths, exactly one `[window]` or `[layer]`, declared `[services].desktop`, `[commands.*]` argv/interval/timeout | Folder-app authors | Stable-candidate | Keys, defaults, path containment and validation errors in [manifest reference](../reference/manifest.md) |
| Luau `bead.ui` constructors, bindings/actions, behavior stores | Folder-app authors | Stable-candidate, except noted below | Element names and documented required properties |
| `bead.kit` components and `bead.widgets` modules | Folder-app authors | Experimental | Component props, embedded source names, and widget persistence remain evolving |
| `desktop.*` stores/actions and `commands.*` result stores/actions | Folder-app authors | Stable-candidate | Only declared service groups/commands exist; typed defaults and action meanings are specified in [services](../reference/services.md) |
| Theme palette roles and `tokens.scrollbar` | Folder-app authors / host theme providers | Stable-candidate for documented roles; optional token group may grow | Missing optional scrollbar keys use neutral defaults; new roles/keys are additive |
| `bead check` / `bead run` folder CLI | App authors / packagers | Stable-candidate | CLI arguments and exit/error behavior; Rust package layout is not part of this contract |
| `bead` facade and documented host entry points | Rust host authors | Stable-candidate after hazard fixes | Prefer façade re-exports and documented modules only |
| Low-level `bead-core`, `bead-render`, `bead-ui`, `bead-wayland`, service/backend internals | Bead maintainers / advanced embedders | Experimental | Re-exported `pub` does not itself promise stability |
| Test, benchmark, proof and wallpaper-process helper APIs | Bead maintainers | Internal (should not be `pub` outside their need) | Keep test support out of the supported facade |

`Arc`/`Polyline` detailed Luau properties are experimental. New optional manifest keys and store keys are additive; unknown keys remain errors so typos are caught.

### Rust workspace surface

Counts below are source-declared public items (`pub` declarations in each crate's `src/**/*.rs`), not a promise that every reachable item is supported. The existing facade re-exports eight crates; Nacre is the only known consumer. Its read-only `git grep` found 2,263 matching lines: `core` 1,210, `render` 429, `ui` 213, `wayland` 210, `services` 128, `backends` 47, `source` 4, `widgets` 1. Counts include tests and repeated references.

| Crate | `pub` items | Consumer-used types/modules (observed) | Classification |
|---|---:|---|---|
| `bead` | 34 | No direct `bead::host` use found; facade is the consuming crate path | Stable-candidate host API; other exports experimental |
| `bead-cli` | 0 | Nacre invokes/depends on no CLI Rust API | Stable-candidate executable contract |
| `bead-backends` | 19 | `Id` (7), `Window` (4), `Action` (10), `Snapshot` (8) | Experimental pending backend contract review |
| `bead-core` | 169 | `Value` (851), `Stores` (30), `source` (137), `profile` (48), `cache` (17) | Mixed: store/source contracts candidate; profiling/cache internal |
| `bead-kit` | 2 | No direct Rust types observed | Internal implementation of Luau kit |
| `bead-render` | 151 | `Primitive` (166), `Palette` (65), `Rect` (53), `Scene` (36), `RendererKind` (19) | Experimental; scene literal hazard |
| `bead-services` | 112 | `ipc` (47), `Desktop` (23), `Snapshot` (10), `Config` (5) | Folder service contract candidate; transport/runtime internals experimental |
| `bead-test` | 35 | No direct Nacre `bead::` references observed | Internal test support |
| `bead-ui` | 285 | `rect` (102), `Session` (68), `Generation` (3), `script` (4) | Experimental except documented `Session`/validation seams |
| `bead-wallpaper` | 29 | No direct `bead::` references observed | Internal until a second host needs it |
| `bead-wayland` | 151 | `UiEvent` (35), `SceneSource` (22), `WindowSpec` (8), `WallpaperProvider` (14), `WallpaperFrame` (7) | Experimental; fix literal hazards before stabilizing |
| `bead-widgets` | 11 | `LayoutStore` / `source` (one module occurrence) | Experimental |

`bead-cli` is a binary package and has no library `pub` items. Public declarations inside private modules and re-exports make source counts approximate; publish review should generate the actual reachable API.

## 2. Breaking-change hazards before stabilization

| Location / observed consumer | Hazard | Smallest fix before publication | Estimated churn |
|---|---|---|---|
| `crates/bead-services/src/runtime.rs:119` `Config` (3 public bools); Nacre has 5 `Config {}` literals | Adding a field breaks external literals | Add `#[non_exhaustive]`; retain `Default`; consumers write `let mut c = Config::default(); c.audio = true;` (non-exhaustive structs cannot be built with struct expressions outside the crate, even with `..Default::default()`) | About 5 Nacre callsites; small |
| `crates/bead-services/src/lib.rs:102` service `Snapshot`; Nacre literal at `status_views_tests.rs:15` | New store fields break exhaustive literals | `#[non_exhaustive]` + `Default`; consumers construct defaults then mutate fields | One observed literal; small |
| `crates/bead-wayland/src/lib.rs:745` `Options` (53 public fields; includes debug/test and product-shaped flags such as `app`/`app_as_pearl`); Nacre uses default update syntax | Every added field can break exhaustive literals; flags mix generic host options with app policy | Keep `Default`, add `#[non_exhaustive]`; later move app-launch policy to a narrow host API | Every consumer `Options { .., ..Default::default() }` literal must become `Default` + field assignment; moderate one-time churn, none afterwards |
| `crates/bead-wayland/src/window.rs:29` `WindowSpec`; 4 Nacre constructors | Field growth breaks literals and bundles policy/callbacks | `#[non_exhaustive]` + constructor/builder with required identity/size/factory and defaults for optional policy | 4 constructors; moderate migration |
| `crates/bead-render/src/lib.rs:445` `Scene`; Nacre has multiple direct literals including `colors_tests.rs:492` | Internal renderer cache/resource fields leak into externally constructed scene | Make implementation fields private and expose `Scene::new`/mutators; interim `Default` + `..Default::default()` only if all fields can safely default | Several Nacre literals; moderate; audit external users first |

For `#[non_exhaustive]`, migrate known consumers in a preparatory revision before the first release. It forbids downstream struct expressions entirely (functional update syntax included) while preserving read access and field assignment on a value obtained from `Default`/a constructor.

## 3. Versioning policy

| Topic | Proposed rule |
|---|---|
| Pre-1.0 Rust crates | Each published crate starts at `0.1.0`. While below `1.0`, a breaking public API change increments the **minor** version (`0.1` → `0.2`); compatible fixes/additions increment patch. Do not imply cross-crate lockstep unless all published crates intentionally share a release train. |
| Stable 1.x | Follow SemVer: breaking API changes only in major releases; additive compatible API and bug fixes in minor/patch respectively. |
| MSRV | Keep workspace `rust-version = "1.99"`; an MSRV increase is a breaking compatibility change for supported users and needs a minor bump while 0.x, major at 1.x, release note, and CI verification before adoption. |
| Changelog | Add root `CHANGELOG.md`, one heading per release (`Unreleased`, then version/date), with `Added`, `Changed`, `Fixed`, and `Breaking` bullets. Link each breaking bullet to migration guidance/spec. |
| Deprecation | Mark deprecated Rust and Luau/manifest names for at least one minor release and 90 days, whichever is longer; state replacement and removal version. Security or data-corruption fixes may shorten the window with explicit rationale. |
| Folder surface version | Reject an optional `bead = "0.1"` manifest key for now. A framework version constraint duplicates the package/lockfile pin, complicates direct `bead check` workflows, and cannot express the exact Git revision used by an embedding host. Revisit only if apps are distributed independently of their host and a resolver can enforce it. |
| Current consumer pin | Nacre pins Bead by exact Git revision in its Cargo workspace today. Keep that pin until released crates exist; after publishing, pin compatible crate versions in `Cargo.toml`/`Cargo.lock`, retaining exact revisions for unreleased work. |

## 4. Publishing readiness (checklist only)

- [ ] Owner approves this policy, crate set, and the first release version; only the owner may flip `publish = false` or publish.
- [ ] Decide package shape. **Recommendation:** consolidate the folder CLI into the `bead` package and publish one supported package first. Publishing only `bead` + `bead-cli` is not currently sufficient: the facade has path dependencies on the internal workspace crates, which crates.io cannot resolve as path dependencies. If not consolidated, publish the complete dependency closure or replace path edges with versioned dependencies.
- [ ] Check names immediately before release. Read-only crates.io API queries for `bead`, `bead-cli`, `bead-core`, `bead-kit`, `bead-render`, `bead-services`, `bead-ui`, `bead-wayland`, `bead-widgets`, `bead-backends`, `bead-wallpaper`, and `bead-test` returned no parseable response in this environment; availability is **unchecked**.
- [ ] Add per-package `description`, `repository`, `readme`, and useful `keywords`; current manifests lack these fields. Confirm package names, categories, include/exclude rules, and dependency/license metadata.
- [ ] Preserve root `LICENSE` and third-party notices. Existing notices include `vendor/system-tray/LICENSE` and `crates/bead-test/fonts/README.md`; verify every packaged asset/source notice is present in the crate archive.
- [ ] Set docs.rs metadata/features explicitly; verify default and documented optional renderer builds on docs.rs-compatible Linux. Do not enable mutually exclusive/heavy backend combinations by accident.
- [ ] Audit `cargo package --list`/crate archives, `cargo deny` licenses, all path dependencies, and the standalone minimal-shell proof after package restructuring.

## 5. Simplicity candidates before stabilizing

| Candidate | Evidence | Estimate |
|---|---|---:|
| `crates/bead-wayland/src/scroll.rs` | B0240 implementation is about 1,052 lines and was just added to the framework surface | ~0 lines until duplication/dead paths are demonstrated; first review API and ownership boundaries |
| `crates/bead-wayland/src/lib.rs` | `Options` combines generic host controls with debug/test and app-shaped flags | Potentially remove/rehome fields; line savings not estimated until API use audit |
| `crates/bead-test/src/lib.rs` and proof-only modules | Test harness/metrics are exported publicly despite serving tests | Unknown; hide any item without an external consumer and count actual deletions in the cleanup unit |

Already removed in the 2026-11 bloat cleanup (B0253 audit):

- B0256: `vendor/fontconfig` and the `gpu`/`vello` feature aliases (the `wgpu` feature stays).
- B0257: `bead::source::{Desktop, HostedNotifications}`, `Notifications::private`, the extra `notification_bus::Server::start*` wrappers, `Mirror::needs_snapshot`, two duplicate monotonic clocks.
- B0258: `bead::host::empty_scene` (use `Scene::empty`), `Session::asset_snapshot`, `Session::local_range_feedback` (use `local_range_feedback_with_label`).

Do not retain temporary duplicate implementations during cleanup. If a compatibility copy is required, record its owner and delete it when the replacement is trusted; no such copy is proposed here.

## 6. Recommended next units

1. Inventory and classify every reachable facade item; hide accidental `pub` exports and record the reviewed API snapshot.
2. Done in [0262](0262-non-exhaustive.md): `#[non_exhaustive]` on `Config`, the service snapshots, `Options` and `WindowSpec` (with `WindowSpec::new`); Bead's own literals are migrated, and 0262 lists the Nacre sites for its pin bump.
3. Encapsulate `Scene` implementation fields behind constructors/mutators; migrate consumer scene literals.
4. Decide the CLI/package consolidation and complete a dry-run package/dependency/license audit (no publication).
5. Approve the stability table, changelog/deprecation rules, manifest compatibility decision, and only then schedule an owner-controlled first release.
