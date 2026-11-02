# B0214 — Independent MIT Bead framework

2026-11-02, phase 1: audit, migration plan, and relicense. Base:
`2d97b3da935e216b1c3971a86912083da72f9ffc`. No repository move or runtime
generalization is performed in this phase. The lead reviews this plan before
cutover. B0212 and B0213 must land before the product history is extracted.

Bead is a general framework, MIT only, copyright siverteh. **Nacre depends on
Bead; Bead never depends on Nacre**, including the permissively licensed
`nacre-protocol`. GPL Nacre Shell is temporarily co-located, not part of the
framework. This supersedes ADR 0004's older dual-license and protocol-dependency
permission; the protocol's own license is unchanged.

## Target boundary

| Bead (MIT first-party code; upstream notices retained) | Nacre (GPL product; separately licensed assets retained) |
| --- | --- |
| `bead-core`, `bead-wayland`, `bead-render`, `bead-ui`, `bead-services`, `bead-test`; facade `bead` and neutral `bead-backends` | `crates/nacre-shell`, including Nacre IPC, daemon contracts, compositor transports, application stores and lifecycle |
| `bead-kit`: reusable components and injected theme tokens; `bead-widgets`: placement, forms, drag/snap and persistence hooks | `shell/`: product views, tokens, widget content, translations, fonts, demos, compositor rules and plugin packs |
| Generic PipeWire, UPower, BlueZ, NetworkManager, MPRIS, notification host; generic wallpaper policy/protocol in existing `bead-wallpaper` | Daemon state/providers, menu ranking, Nacre settings IDs, Orient integration, niri-ipc/libmpv adapters and renderer executables |
| Plugin loader/manifest/capability gate, VM budgets, registry and generic CLI core; `examples/minimal-shell` and framework specs | Plugin content/defaults/index, `nacre plugins` frontend, packaging, installer/switch, QML parity and product acceptance |

`bead-kit` and `bead-widgets` are target modules/crates, not existing packages.
Extract existing general components instead of building parallel implementations.
Keep the facade, neutral backend vocabulary and `bead-wallpaper` initially:
their code is already generic and permissive. Package consolidation is a later
API choice, not a prerequisite for removing Nacre. `bead-test` owns reusable
nested input/capture support; Nacre keeps scenarios and product goldens.

The plugin research informs ownership, not new implementation scope. Reuse
`bead-ui/src/plugins.rs` and the existing hello plugin. Preserve explicit
capabilities, bounded VMs, install-versus-enable separation, pinned sources,
and app-supplied roots. Neither Nacre plugin content nor its palette provider
belongs in the loader. Do not invent another plugin runtime or ranking engine.

## Framework coupling audit

Read-only searches covered all tracked `crates/bead-*` **and** `crates/bead`
files: case-insensitive Nacre/nacrectl/nacre-link, manifests, `shell/`, config
and XDG paths, environment reads, compile-time includes, app IDs, D-Bus names,
locale/i18n, token/font/asset loading. Tables group repeated sites in the same
file. Line numbers refer to the base, whose source changes here are SPDX only.

| Files / sites | Finding | Migration disposition |
| --- | --- | --- |
| All nine `bead*` manifests | No Nacre crate/git dependency. Every crate is unpublished. `bead` uses core/render/ui/wayland/services/backends; render uses core; ui uses core/render; wayland uses render/core/backends/services; test uses core/ui/render. Other dependencies are external libraries. | Keep direction; independently check each graph, including facade and all features. No `nacre-protocol` exception. |
| `bead-services/src/notification_bus.rs:73,165,692` | Server vendor `Nacre`; private policy interface `org.nacre.Bead.Notifications`. Standard freedesktop notifications interface is separate. | Host-supplied server identity; neutral Bead policy interface. Nacre owns any compatibility bridge for the old private interface. |
| `bead-services/examples/notification-client.rs:76,133` | Uses the same Nacre policy interface. | Change with service API and private-bus proofs; allow target selection where useful. |
| `bead-ui/src/accessibility.rs:15` | Accessible window root is `Nacre Shell`. | Inject host application label through UI/session configuration; preserve Nacre label in the app. |
| `bead-wayland/src/pearls.rs:207-211,875,989,1740,1917,2117` | `bead.pearl.pending`, `bead.pearl`, `bead.pearl.dock.{id}`; comment requires product `shell/niri-pearls.kdl` opacity rule. | Inject pending/visible app IDs with placement/handoff config; internal animation IDs may stay neutral. Product adapter supplies matching niri rules. Test hidden/admitted handoff before cutover. |
| `bead-render/src/lib.rs:3192` | Test loads `../nacre-shell/assets/fonts/Outfit-Variable.ttf`. | This exact thin-digit regression needs Outfit: move the product-specific case to Nacre, retain a generic raster assertion using framework test fonts. Do not silently substitute a font and lose coverage. |
| `bead-render/src/lib.rs:3525-3526` | Test path `../../shell/sample.png`. | Independent small framework fixture, with license/provenance. |
| `bead-ui/src/markup_tests.rs:1912,1917,1922,1925,2034,2039,3281,3396,3498` | Compile-time includes of product `shell/palette.json`, `sample.png`, `kit/behavior.luau`, `kit/main.luau`. | Author neutral palette/image/kit fixtures in framework test data; keep matched Nacre rendering tests with the product. These includes block deleting `shell/`. |
| `bead-ui/src/frontend.rs:721-726`, `assets/size-scale.json`, `assets/kit.luau:24,148,163,720`; `text_style.rs:12-31` | Embedded scale values mirror Nacre; symbol/mono families assume Material Symbols Rounded / JetBrainsMono Nerd Font. Existing alias/size-offset support is generic. | Host supplies token/family map and optional defaults; no Nacre file synchronization required. Nacre chooses exact existing values. Reusable kit geometry stays configurable. |
| `bead-ui/src/i18n.rs:53-60` | Injected built-in English table via `set_builtin`; no embedded Nacre catalog or daemon route. | Keep generic translation interface; application owns catalogs, namespaces and refresh transport. |
| `bead-ui/src/plugins.rs:269-280,300-316` | `Config::for_app(root, app)` derives XDG data/config locations; unconfigured roots use their own `plugins/`. | Existing seam is general. App supplies roots/identity; preserve it rather than introducing a Nacre default. |
| `bead-wayland/src/window.rs:29-43`; `bead-backends/src/lib.rs:39`; `bead-wayland/src/lib.rs:738-820` | WindowSpec accepts app ID/title; Options namespace defaults to `bead-dev`. | Keep explicit host configuration; minimal-shell selects its own namespace/app IDs. No Nacre IDs found outside pearl-policy coupling above. |
| `bead-render/src/lib.rs:1079-1082`; `bead-wayland/src/runtime.rs:2106-2108`; `bead-services/src/bus.rs:81` | `LC_ALL`/`LANG`, `HYPRLAND_INSTANCE_SIGNATURE`/`NIRI_SOCKET`, `DBUS_SYSTEM_BUS_ADDRESS`. Remaining `BEAD_*` reads are diagnostics/proof/test controls. | Standard environment and framework diagnostics; retain/document. No Nacre env var, `~/.config/nacre`, `nacrectl` or `nacre-link` literal in framework crates. |
| `bead-services` bus implementations/tests | Standard UPower, BlueZ, NM, login1, MPRIS, KDE SNI/DBusMenu and freedesktop names. | Generic services remain; own no Nacre daemon topic IDs. Keep test buses isolated. |
| `bead-wallpaper/src/{lib,policy,protocol,supervisor}.rs` | Neutral JSON-lines protocol, process command injection, pause/release policy; no product setting, path, daemon or GPL renderer. | Retain existing generic crate; Nacre owns its renderer binaries and configuration. |
| `bead-backends/src/lib.rs:48`; `bead-core/src/pearl.rs:62,399` | Comments tie capability names/default tuning/persisted state to Nacre. | Describe general contracts; preserve existing application state compatibility in Nacre. Do not change stored state format merely to rename provenance. |
| `bead-wayland/src/intent.rs:24,41,138,159`; `edges.rs:121,790`; `bead-ui/src/themed.rs:52`; `assets/kit.luau:39,150,197,617` | Nacre QML type/control/logo provenance in comments. | Generalize descriptions; retain necessary attribution. No GPL implementation may be copied into framework code. |
| `bead-core/SOURCES.md:39-41`; `bead-services/README.md:39`; `bead-test/README.md:32` | Product adapter history, planned move into nacre-daemon, and `cargo test -p nacre-shell` guidance. | Move product migration notes; replace framework instructions with independent proofs. |

Paths above abbreviate the containing `crates/` prefix. No absence claim means
that all framework API assumptions are already generalized: identity, tokens,
test assets and the private policy interface still need the commits below.

## Complete repository ownership inventory

This is a partition of **every tracked file** in the requested locations at the
base (1,963 files). Subtree rows include every descendant, including binary
evidence, manifests, locks and notices; more specific rows override broader
ones. F = framework, P = product/history to Nacre, S = split at the stated
boundary. These are migration destinations, not changes made in this phase.
Third-party licenses travel with their files regardless of destination.

| Root file (13 at base) | Class | Action |
| --- | --- | --- |
| `.gitignore` | S | Keep generic target/cache rules; product-specific rules follow moved paths. |
| `AGENTS.md` | S | Bead framework/license rules stay; Nacre workflow, product/memory gates move. |
| `Cargo.toml` | S | Bead-only workspace; Nacre adds product crate and pinned Bead dependencies. |
| `Cargo.lock` | S | Separate locked graphs, regenerated and reviewed after workspace edits. |
| `FEASIBILITY.md` | P | Archive experiment measurements with product; new framework measurements stay in Bead. |
| `LICENSE-APACHE` | F, obsolete | Delete now; no retained file needs this root copy. Preserve font's adjacent Apache notice. |
| `LICENSE-GPL-3.0` | P | Keep through transition, move with GPL product/references. |
| `LICENSE-MIT` | F | MIT copyright siverteh. |
| `LICENSE.md` | S | Explicit current boundaries; framework-only after cutover, upstream notices retained. |
| `README.md` | S | Independent Bead introduction now; product build/install guide moves with product. |
| `deny-framework.toml` | F | Permissive dependencies, no Nacre git source. |
| `deny.toml` | S | Keep GPL workspace allowance until removal; product exceptions move to Nacre. |
| `rust-toolchain.toml` | F | Retain Bead pin; Nacre reconciles its own toolchain before adding the product. |

| Exact subtree/file set (count at base) | Class | Action / exceptions |
| --- | --- | --- |
| `.github/workflows/ci.yml` (1) | S | Framework jobs remain; Nacre shell, schemas, packaging, motion wallpaper and product capture proofs move to Nacre CI. |
| `tools/check-service-boundary.py` (1) | F | Generic headless service graph guard. |
| `tools/build-test-font.py` (1) | F | Builds bead-test's pinned renamed symbol font; not the Welcome font baker. Keep upstream notice/hash checks. |
| `tools/check-luau.py` (1) | S | Parameterize bundle/type-generator/assets inputs; neutral SDK/analyzer in Bead, Nacre bundle checks in Nacre. Currently copies `shell/`. |
| `tools/sync-size-scale.py` (1) | P | Nacre scale mirror synchronization. Replace framework need with host token injection. |
| `tools/bake-welcome-demo.py`, `tools/i18n-sync.py` (2) | P | Product demo/catalog generation. |
| `tools/daily-acceptance/**` (11) | P | All scripts/configs/protocol/C pointer helper and README: product acceptance. A later generic input helper can move into bead-test, without product scenarios. |
| `spikes/**` (132) | P | All historical experiment/proof tools, fixtures, tests, outputs, reference QML and both spike crates archive with Nacre. Includes `blank-layer/LICENSE-SCTK`. Promote only reusable capture/input/service machinery into bead-test before retiring any needed framework proof. |
| `packaging/**` (5) | P | All: Arch PKGBUILD, `stage.sh`, `bin/bead-shell`, `applications/nacre-widgets.desktop`, `tests/test_stage.py`. They package nacre-shell plus product assets/GPL text. |
| `vendor/fontconfig/**`, `vendor/system-tray/**` (25 combined) | F | Framework transitive font/service patches with upstream MIT notices and patch provenance; Nacre also needs a root Cargo patch for whichever git dependency uses them. |
| `examples/plugins/hello/**` (2) | F | Neutral loader example; MIT manifest and source. |
| `examples/README.md` (1) | S | Existing text is product previews; replace with minimal-shell/plugin instructions and move historical guide. |
| `examples/hyprland.lua`, `hyprland-backend.lua`, `niri.kdl`, `niri-backend.kdl` (4) | P | Despite generic compositor names they launch `nacre-shell`; backend versions also launch a QML fixture. New minimal-shell configs are independent. |
| `examples/*.png` (15) | P | Historical framework/product/QML screenshots follow experiment evidence; use new neutral example images for Bead. No blanket MIT claim over depicted assets. |
| `docs/decisions/0001-architecture.md`, `0002-authoring-format.md`, `0003-services-outside-view.md` (3) | S | Retain framework architecture, reload/format/services contracts; move Nacre integration examples and product gates. |
| `docs/decisions/0004-structure-licensing-generalization.md`, `0005-single-ui-process.md` (2) | S | Keep general licensing/generalization and host/process principles; product process policy/history to Nacre. Amend superseded direction now. |
| `docs/COMPONENTS.md`, `docs/PREVIEW.md` (2) | S | General API/token/preview docs remain; remove Nacre scale synchronization, product build/lifecycle/tray and preview assumptions. |
| `docs/ROADMAP.md`, `docs/STATUS.md` (2) | P | Historical feature gates/milestones to Nacre; replace Bead guides with framework scope/status. |
| `docs/development/ci.md` (1) | S | Retain independent framework CI procedure; product CI instructions to Nacre. |
| `docs/research/**` (35) | P | All historical research/provenance/dependency inventories and measurements; do not rewrite historical license facts. Framework architecture references may link to archived history. |
| `docs/specs/**` excluding `evidence/` (200) | P/S | Archive all historical feature records and index with Nacre. Retain/rewrite framework contracts from generic portions (notably 0025 sessions, 0032 i18n, 0035 stores, 0045 windows, 0135 scrolling, 0186 budgets, 0203 plugins). New Bead specs index lists independent framework contracts; 0214 stays as the split decision. |
| `docs/specs/evidence/**` (1,503) | P | All experiment/product evidence, including CI logs and rejected patches. Preserve immutable facts and license provenance. |

Additional tracked trees, outside the requested inventory, must also be handled:

| Set | Class | Action |
| --- | --- | --- |
| `crates/bead*` (129 files; nine manifests) | F | Coupling table above; framework assets and upstream test-font notices remain. |
| `crates/README.md` | S | Framework crate map remains; product description moves. |
| `crates/nacre-shell/**`, `shell/**` | P | All product runtime/views/assets/tests/fixtures/goldens. Extract generic widget/form/drag/placement code first, without product content. |
| `scripts/**` (11) | P | All current dev/demo/try-bead and ui-switch/systemd-validation scripts exercise the product. New minimal-shell runner uses bead-test primitives. |
| `ssh-placeholder/**` (718) | P | Entire tracked mirror of shell/spikes/tools, including branding and GPL references. Archive in product history, then remove from Bead with the product. No hidden product copy remains. |

Conservative archive ownership does not mean generic spike code is intrinsically
product-specific. It preserves experiment history intact while Bead gets a small,
reviewed, self-contained harness rather than the whole Nacre proof suite.

## Migration commits and estimates

Estimates are engineering days including review and remote CI, not measured
throughput. Host availability and owner acceptance can extend elapsed time.
Every commit uses `[no-publish]`; no crates, tags or releases are published.

| Order / commit boundary | Concrete work and acceptance | Estimate |
| --- | --- | --- |
| 0a / this phase: audit and plan | This inventory, target ownership, installation contract and lead review gate. No move. | 0.5–1 d |
| 0b / this phase: MIT only | All nine Bead manifests, workspace default, first-party SPDX/prose/notices, examples/spikes/tools; preserve GPL product and upstream notices; cargo-deny independent graphs. | 0.5 d |
| 1a / independent identities | Add app configuration for accessibility label, notification server identity and pearl pending/visible IDs; neutral private policy interface. Minimal call-site updates in nacre-shell keep its exact identities and compatibility behavior. Private-bus and handoff tests on both compositors. | 1–2 d |
| 1b / test independence | Replace compile-time shell palette/image/kit fixtures with authored generic data; move Outfit-specific regression into Nacre product tests; framework tests succeed with product trees unavailable. | 0.5–1 d |
| 1c / theme and widget boundaries | Host-injected tokens/font families; extract existing generic kit/widgets forms/placement/drag/persistence interfaces, while Nacre supplies contents, storage paths and routes. IPC remains app-supplied Source/service/backend traits; do not add nacre-link or daemon knowledge. Keep store/reload/render layers independent of frontend. | 1–2 d |
| 2 / minimal-shell | Add unpublished `examples/minimal-shell` Cargo package/member using Bead only, own Luau, palette, tokens, app ID and test fixtures. Existing `bead --example minimal` only draws a rectangle: retain as smoke or replace with no duplicate sample after new host works. Demonstrate view+input+reload and hello plugin on private nested niri and Hyprland with no Nacre tree/helper/daemon/socket/config/font bundle. `--check` works headlessly. | 1–2 d |
| 3a / prepare Nacre source contract | After B0212/B0213 land, freeze source and shell-tree IDs; finalize export paths above. Prepare Nacre workspace/git pins, root vendor patches, CI/staging/install tests using matched three-bin + shell artifacts. Keep old Bead source build usable. No install/switch occurs merely by changing repos. | 0.5–1 d |
| 3b / history import in Nacre | Import filtered product history into `crates/nacre-shell/` and `shell/`; product tools/scripts/docs under `bead-shell/{tools,scripts,docs}/` to avoid overwriting Nacre tools/spec numbers, packaging reconciled under existing Nacre packaging. Reconcile relative/compile-time asset paths. Tests/build/packaging accept the exact Bead pin. | 1–2 d |
| 3c / Nacre cutover support | Nacre CI, package stages, source-install artifact producer and lead preview build all consume Nacre product sources plus pinned Bead. Fake-HOME installer, systemd rehearsal, rollback and nested product parity pass. Lead handles preview tooling and any separately authorized live installation. | 1–2 d |
| 4 / remove product from Bead, independent CI | Only after 3c passes, delete exported product, mirror, packaging and historical proofs/docs from Bead; keep neutral example/SDK/services/harness checks. Build/test all framework roots without a Nacre checkout. Nacre stays on the exact tested Bead rev; a later pin may select this cleanup commit. | 0.5–1 d |
| 5 / closure, not a live switch | Confirm installer still accepts old and new matched bundles, installed versions/rollback remain untouched, and dependency direction guard fails on Nacre imports. Remove transitional source duplication. Lead accepts final repository boundary. | 0.5 d |

Total remaining implementation estimate: **7–14 engineering days** (steps 1–5,
before any overlap). No optional de-Nacre runtime commits in phase 1:
API/call-site and compositor compatibility changes require remote validation.

### Preserve history without rewriting public branches

Use **git filter-repo on a disposable clone**, with an explicit path selection
from this inventory and path renames for product tools/docs. Record the final
source commit, original-to-filtered commit map and source tree IDs in the import
commit. Verify imported product bytes and authors against the frozen export;
include GPL and asset notices. Import the filtered branch into Nacre with a
normal unrelated-history merge, resolving root/license/build files deliberately.
Neither public repository's existing commits are rewritten or force-pushed.

filter-repo can select several independent paths and preserve their individual
history in one import. A subtree import of the entire Bead root would bring
unwanted framework history/content; a synthetic staging prefix would obscure
the selected paths. Never filter the working Bead or Nacre repository in place.
Old history retains its historical license grants; this phase declares the
framework's current distribution MIT only, without altering past commits.

Use `crates/nacre-shell` and top-level `shell/` in Nacre: this preserves the
product's relative `../../shell` paths and package asset layout. Check for
collisions at the *then-current* Nacre target before importing. Framework
dependencies in Nacre use `git = "https://github.com/nacre-desktop/bead"` and
the same full 40-character `rev` for each package, with `Cargo.lock` committed.
Local development uses uncommitted `[patch."https://github.com/nacre-desktop/bead"]`
path overrides for all participating Bead packages. Bead's root crates.io vendor
patches do not propagate to dependents: Nacre must own equivalent root patches
or deliberately adopt upstream fixes before the build is considered matched.

### Build, CI, packaging, install and preview contract

Nacre integration was inspected read-only at its locally available `origin/main`
`816957914b3c95872f942a9bd6d7bb0b4ac8a64e`, not its older checked-out files.
Revalidate that target when executing the plan.

| Integration point | Required change / invariant |
| --- | --- |
| Nacre `Cargo.toml`, `Cargo.lock` | Add shell member/binaries, exact Bead pins, compatible Rust pin/features/profiles and root patches. No Bead dependency on helper/protocol. |
| Nacre `.github/workflows/check.yml`, `tools/check.py` | Keep current full checks; add product workspace/tests, generated Luau/schema, shell staging and installer proofs against the exact Bead rev. Optional non-blocking Bead-main compatibility job never changes the shipping pin. |
| Bead `.github/workflows/ci.yml` | Retain fmt, warnings-denied clippy/tests, all renderer configurations, service graph, independent cargo-deny roots/private buses, SDK and neutral example proofs. Move nacre-shell/trace tests, shell Luau, packaging, video renderer smoke and product capture suites into Nacre. No silent coverage deletion. |
| Nacre `packaging/arch/{recipe.py,stage.py,check.py,test-installed.py,PKGBUILD}`; incoming Bead `packaging/**` | Reconcile shell package/build/staging at product source; retain executable names, asset paths, notices and side-by-side fallback. Update package checks and source artifact production together. No package publication in this work. |
| Nacre `tools/releases.py:878-882`, `tools/ui_shell_switch.py:99-208,593-697` | Keep `nacre ui-shell install --binary FILE --shell DIR` and status/bead/qml/rollback API. It requires `nacre-shell`, `bead-wallpaper-video`, `nacre-wallpaper-decode`, `shell/palette.json` and `shell/bar`. Installed transaction copies remain immutable; source repo location must not affect them. |
| Same switch code `:147-177,189-208` | Continue versioned bundle copy and launch wrapper with existing UI/daemon socket paths; complete journal only after health validation. Refuse install while Bead is active or a transaction incomplete. Preserve rollback and single-service switch semantics. |
| Nacre `tools/tests/test_ui_shell_switch.py`, `tools/tests/rehearse_ui_shell_systemd.py`, `nacre/shell-tools/install.py` | Exercise old/new matched bundle layouts in private HOME, install refusal, failure recovery, selection and rollback. Preserve active-Bead-aware deployment validation. Never run installers against real HOME in a worker task. |
| Lead `refresh-bead-preview` (read-only in phase 1) | Lead changes source/tree manifest inputs: build nacre-shell from Nacre; get shell/scripts/product fixtures there; archive pinned Bead separately for dependency builds. Preserve output bundle `bin/{nacre-shell,bead-wallpaper-video,nacre-wallpaper-decode}` + `shell/`, private nested launch and rollback. Refresh key must include Nacre product tree **and** Bead rev, not Bead HEAD alone. |

The install command accepts artifacts, not a Bead checkout. Therefore keep the
artifact shape unchanged across the repository move: an existing installation
keeps its copied files and needs no migration. Do not remove the old producer
until the new Nacre producer and staging/installer proofs pass. QML retirement
or a different active-shell choice is outside this unit.

## Risks and stop conditions

| Risk | Required protection / stop |
| --- | --- |
| Owner's live install breaks or switches unintentionally | No install/service/config/preview execution in this phase. Later lead uses existing journaled path only after review and separate live authorization; old installed bundle remains recoverable. |
| Concurrent shell edits/history lost | Land B0212/B0213 first; freeze export afterward, include their history and audit diff for late edits. No mid-feature move. |
| Source/archive gaps between repositories | Import and validate in Nacre before deleting in Bead. Transitional duplicate product sources expire **7 days after the Nacre replacement is trusted**, preferably removed in the same accepted cutover batch. No permanent dual source of truth. |
| Hidden relative paths / fixtures / third-party assets | Framework tests with product directories absent; independent minimal host; full product CI and matched nested evidence. Preserve OFL/Apache/CC0/GPL notices; do not relicense copied references or fonts. |
| GPL or Nacre coupling leaks into framework | Independent all-feature deny graphs and source/dependency boundary guard, including facade. Move niri-ipc/libmpv and Nacre IPC only to product. Dependency Apache/BSD/etc allow-lists remain permitted, not framework license alternatives. |
| Application identity or D-Bus clients regress | Update client/service together, keep app identity/rules at edge, test private bus and compositor handoff; no live experiment as a substitute. |
| New crate/API churn masks missing portability | Extract existing code only, prove one independent consumer; avoid speculative service/plugin abstractions. Treat missing remote tests as unverified. |

## Phase 1 license evidence and validation

`git log --format='%an <%ae> | %cn <%ce>' -- crates/bead-* crates/bead`
reports only siverteh's approved noreply identity for both author and committer.
No other human author found. Co-author trailers name Claude Opus 5.5, Claude
Sonnet 5.5 and Codex Luna (agent work); they are not independent copyright
holders claimed by this project. Third-party files still retain their authors.

All Bead manifests (including already-MIT bead-test and the facade) are MIT;
the workspace default is MIT, with explicit GPL-3.0-only on nacre-shell. Source
SPDX headers, first-party tools/spikes/examples and framework prose now agree.
The tracked placeholder's first-party spike/tool SPDX headers agree too; its
GPL shell/reference content is unchanged. The root MIT text and bead-test MIT
text name siverteh. Root Apache text is removed: vendor notices are standalone
MIT, SCTK notice is MIT, and the Apache test font retains its adjacent full
`crates/bead-test/fonts/Apache-2.0.txt` (IBM Plex retains `OFL.txt`). Historical
evidence, external license inventories and nacre-protocol's own dual license
remain factual records, not stale Bead license declarations.

No runtime behavior, product asset, installed bundle or external repository is
changed. Local `git diff --check` passed; single-file TOML parsing passed for
workspace/crate/spike manifests, both deny configs and the hello manifest.
Neither rustfmt nor luau-analyze is on the local PATH. Both required one-shot
SSH probes failed (CI host: network unreachable; capture host: timeout).
No local build, test, nested session or cargo-deny execution was substituted.

The lead must run **every step of `.github/workflows/ci.yml`** against the exact
pushed revision in the prescribed remote Ubuntu 24.04 container, using its
pinned tool downloads and private buses, including:

| Check group | Exact command / workflow step still required |
| --- | --- |
| Rust/static dependency guard | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -j2 -- -D warnings`; `python3 tools/check-service-boundary.py` |
| Tests/features | `cargo test --workspace --locked -j2`; `cargo test -p nacre-shell --features trace --locked -j2 memory_policy::tests::`; `cargo check -p nacre-shell --no-default-features --locked -j2`; `cargo check --workspace --all-features --locked -j2` |
| Facade/schema | `cargo run -p bead --example minimal --locked -j2 -- --check`; `cargo build -p nacre-shell --no-default-features --locked -j2`; `target/debug/nacre-shell --check shell/ui/main.luau`; pinned Luau 0.742, then `python3 tools/check-luau.py --analyzer "$RUNNER_TEMP/luau/luau-analyze" --binary "$PWD/target/debug/nacre-shell" --check-generated` |
| Capture-window regressions | Exact suite and single-file compilation lists below |
| Product package/video | `python3 -m unittest discover -s packaging/tests`; `cargo build -p nacre-shell --bin bead-wallpaper-video --locked -j2`; `python3 spikes/wallpaper-video-smoke.py --engine target/debug/bead-wallpaper-video --expect-presentation shm` |
| Service/private buses | `python3 spikes/services-check.py`; exact private SNI/DBusMenu and clock bus commands below |
| License/source policy | `cargo deny check`; `for manifest in crates/bead-*/Cargo.toml crates/bead/Cargo.toml; do cargo deny --manifest-path "$manifest" --all-features --config deny-framework.toml check; done` |

```sh
for suite in test_dashboard_capture_windows.py test_menu_engine_trace.py \
  test_b0003_arm.py test_b0002_arm.py test_laptop_reference_capture.py \
  test_foundation_preview.py test_fake_system.py test_fullscreen_lips.py \
  test_pearl_release_proof.py test_detach_ghost_proof.py test_launcher_icons_proof.py \
  test_launcher_memory_proof.py test_scrolling_proof.py; do
  nice -n 10 python3 -m unittest discover -s spikes/tests -p "$suite"
done
nice -n 10 python3 -m py_compile spikes/capture_safety.py spikes/measure.py \
  spikes/bar-matrix.py spikes/dashboard-residency.py spikes/launcher-proof.py \
  spikes/b0002-arm.py spikes/laptop-reference-capture.py \
  spikes/laptop-reference-artifacts.py spikes/parity-compare.py spikes/fake-bluez.py \
  spikes/fake-power.py spikes/fake_system.py spikes/foundation-preview.py \
  spikes/menu-engine-proof.py spikes/menu_engine_trace.py spikes/b0003-arm.py \
  spikes/tests/test_dashboard_capture_windows.py spikes/tests/test_menu_engine_trace.py \
  spikes/tests/test_b0002_arm.py spikes/tests/test_laptop_reference_capture.py \
  spikes/tests/test_b0003_arm.py spikes/tests/test_fake_system.py \
  spikes/tests/test_launcher_memory_proof.py
dbus-run-session -- bash -c 'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" BEAD_PRIVATE_SERVICES_TEST=1 BEAD_FIXTURE_UPDATE=1; cargo test -p bead-services --test tray_private_bus --locked -j2 -- --ignored --nocapture'
dbus-run-session -- bash -c 'export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" BEAD_PRIVATE_SERVICES_TEST=1; cargo test -p bead-services --test clock_private_bus --locked -j2 -- --ignored --nocapture'
```

All remote commands use nice priority 10 and Cargo two jobs. Runtime validation
is deferred, not passed; no new performance or memory claim is made.
