# B0203 Plugins v1

Status: candidate on `task/claude-b0203-plugins-v1-20261030`; lead gate pending.
Owner-approved direction (2026-10-29, research `install-plugins-20261029`, prior art Noctalia v5).

## Format
A plugin is a directory named after its id, holding `plugin.toml` and Luau files.

```toml
id = "hello"                 # [a-z][a-z0-9-]{0,63}; equals the directory name
name = "Hello"
version = "0.1.0"
author = "..."               # optional, like description and license
description = "..."
api = 1                      # plugin API level, see below
capabilities = ["topic:bar.updates.", "action:updates.open", "network"]

[[widget]]                   # one or more
id = "hello"                 # the plugin id, or "<id>.<name>"
entry = "widget.luau"        # relative .luau path inside the plugin
accent = "secondary"         # optional default palette role
title_id = "..."             # optional i18n ids (first-party); else name/description
description_id = "..."

[settings.greeting]          # optional schema: toggle | choice | number | slider | text
type = "choice"
label = "Greeting"
options = ["Hello", "Hi"]
default = "Hello"
```
Unknown fields are errors. Limits: 64 plugins, 64 Luau files per plugin, 256 KiB per file.

## API level 1
- The host implements levels `API_MIN..=API_LEVEL` (both 1). Levels only grow and stay
  cumulative; a manifest outside the range is listed with an error and not loaded.
- A widget entry module returns a factory `function(api) -> package`. `api.ui` and `api.kit`
  are the Bead UI and kit, `api.plugin` is `{id, settings}` (plugin-level values), `api.app`
  is whatever the host application exposes (Nacre: `context`, `measure`, `visual`; app helpers
  are not covered by the API level). The package is the widget registry's existing package shape
  (`id, icon, category, data, settings, defaults, budget, forms`), so plugin widgets use the
  same bar/desktop forms, tokens and palette as built-ins.
- A plugin may `require` only `bead.ui`, `bead.kit` and files inside its own directory.

## Capabilities
| Capability | Meaning | Enforced by |
|---|---|---|
| `topic:<prefix>` | read stores whose name starts with prefix | `resolve_for_check` (by calling file) and `ui.plugin_gate` (every binding in the rendered subtree) |
| `action:<prefix>` | emit actions whose name starts with prefix | `ui.plugin_gate` (every `on_*` action in the rendered subtree) |
| `network`, `files`, `exec` | declared and shown to the user | denied by omission: API 1 has no host function for them |

Framework interaction state (`bead.hovered`, `bead.focused`, `bead.pressed`, `bead.focus_visible`,
`bead.viewport.*`, `bead.preview.*`) is always readable. Checks happen in Rust host functions;
Luau's sandbox is a stability boundary, not a security one (a plugin can still call shared app
helpers that read state on its behalf). The Plugins page says so: "Luau sandboxing is not a
security boundary: only enable plugins you trust."

## Lifecycle
1. **Discover**: system dirs (default `<shell root>/plugins`, bundled first-party plugins) and the
   user dir (default `$XDG_DATA_HOME/<app>/plugins`). A user plugin with a bundled plugin's id is
   listed as a duplicate error.
2. **Validate**: manifest parse + checks above, directory name = id, entries exist.
3. **Enable/disable**: bundled plugins are enabled by default, user plugins are disabled until
   enabled. Choices persist in the state file (default `$XDG_CONFIG_HOME/<app>/plugins.json`:
   `enabled`, plugin-level `settings`, `faults`, `updates`, `notice`).
4. **Load**: a bundle opts in with `"plugins": true` in `bundle.json` (Nacre: bar, Widgets app,
   drag ghost, editor). Its generation then gets the active plugins' Luau under `plugins/<id>/`
   plus a generated `plugins/index.luau` listing every discovered plugin (valid or not, enabled
   or not, error text, capabilities, settings schema and values). The shell registry registers
   each active plugin widget into the catalogue like a built-in.
5. **Unload**: disabling (or a fault) removes the plugin from the next mount; the watcher follows
   the plugin dirs and the state file, so every view reloads without the plugin and its registry
   entries are gone. Existing layout instances of an unknown widget stay opaque (unchanged rule).

## Failure model
A broken plugin never breaks the shell:
- invalid manifest or unreadable files: listed with the error, not mounted;
- error while loading its entry, or a registry rejection (duplicate id, bad package): skipped,
  error recorded;
- error or capability denial while rendering: the registry renders nothing for that widget
  (`pcall`) and records the fault;
- a recorded fault (`PLUGIN_ERROR id=... <message>` on stderr, `faults` in the state file) keeps
  the plugin off until the user enables it again; the Plugins page shows "Disabled after an error".
- A plugin that exceeds the 50 ms markup budget fails that one build (validate-then-swap keeps the
  previous UI), is recorded as faulted and is not mounted on the next reload.

## Sources and updates
- Local folder: copied into the user dir (no symlinks, ≤512 entries, ≤1 MiB per file).
- Git URL (`https://`, `ssh://`, `git@`, `file://`): shallow clone into the user dir.
- Both arrive disabled. Auto-update is OFF: "Check for updates" fetches git plugins and marks
  those with a newer upstream ("Update available" badge); "Update" applies it on request. Git and
  copies run on a worker thread; their outcome is written to the state file (`notice`).

## UI
Widgets app → sidebar "Plugins" (a route `@plugins` rendered by `widgets/app/plugins.luau`):
trust note, source field with Add and Check for updates, and one card per plugin with name,
version/author/origin, description, declared access in plain words, enable switch, settings gear
(plugin-level settings through the existing `settings-form` rows: toggle, choice, number), update
badge and error line. Touches to the app shell (B0198b area) are limited to: one sidebar row, the
route switch, appending plugin widget ids to the sidebar list, and three stores
(`plugins.source`, `plugins.message`, `plugins.scroll`) in `behavior.luau`.

## Code
- Framework (MIT/Apache, `bead-ui`): `plugins.rs` (manifest, discovery, state, mount, faults,
  install/update), frontend gates (`require`, `resolve_for_check`, `ui.plugin_gate`,
  `ui.plugin_fault`), `bundle.json` `plugins` flag, watcher extra paths, `Session::root`.
  New dependency: `toml` (MIT/Apache, already in the lock graph).
- Nacre edge (GPL): `nacre-shell/src/plugins.rs` (default paths for app `nacre`, `plugins.do`
  actions), registry loading in `shell/widgets/registry.luau`, the Plugins page.
- First-party conversion: the Updates widget moved from `shell/widgets/updates/widget.luau` to
  `shell/plugins/updates/` (plugin.toml + factory). The old path is deleted; goldens unchanged.
- Example: `examples/plugins/hello` (a bar greeting with a choice and a toggle setting).

## Not in v1
Launcher providers and panels as plugin parts, a `bead plugin` CLI, per-plugin VMs and memory
budgets, process sandboxing for future exec/network host functions, plugin removal from the UI
(delete the folder), curated index.

## Evidence
- `cargo test -p bead-ui plugins::` (manifest validation, persistence + unload, capability
  denial for topics/actions/requires, broken plugin isolation, local install).
- `cargo test -p nacre-shell plugins::` (bundled updates plugin + Plugins page; example plugin
  enable → render with a plugin setting → disable → gone; a plugin reading an undeclared store
  is switched off with its error shown while the app keeps rendering).
- Updates goldens and bar parity goldens unchanged under the full workspace test run.
