# ADR 0002 — Luau markup over a replaceable front end

Date: 2026-10-13. Status: **chosen under overnight authorization; owner may veto**.
Scope: remove absolute-coordinate JSON as the primary authoring format. No Qt parser.

## Same panel, three forms

Each has the same800×480 panel, padding12, a search field,20%-width sidebar,
keyed navigation rows, a scroll store, three-column app grid, reusable typed AppTile,
imports and named actions. No hand-listed UI rows or visibility expressions.
All use the same13-line [behavior declarations](../../spikes/authoring/behavior.luau).
Data/model producers are intentionally unspecified here, identical for all options;
this is syntax/authoring comparison, not a rendered parity claim.
Physical lines include strict-mode/header/closing lines; no blanks were compressed
away. Counts are31/27/34 UI,44/40/47 including the common behavior.

<table><thead><tr><th>Existing language/runtime</th><th>Existing structure parser + behavior</th><th>New UI grammar + behavior</th></tr></thead><tbody><tr>
<td><strong>Luau markup — 31 lines</strong><pre>--!strict
local ui = require(&quot;bead.ui&quot;)
local Behavior = require(&quot;./behavior&quot;)
local function AppTile(p: { item: Behavior.App }): ui.Node
    return ui.Button {
        id = &quot;tile&quot;, text = p.item.name, selected = p.item.selected,
        on_activate = ui.action(&quot;select&quot;, p.item.id),
    }
end
return ui.Panel {
    id = &quot;launcher&quot;, width = 800, height = 480, padding = 12,
    ui.Column { gap = 8,
        ui.TextInput { id = &quot;search&quot;, text = ui.bind(&quot;launcher.query&quot;),
            placeholder = &quot;Search applications&quot; },
        ui.Row { gap = 12, grow = 1,
            ui.Column { width = &quot;20%&quot;, gap = 4,
                ui.Repeat { id = &quot;nav&quot;, model = ui.bind(&quot;launcher.navigation&quot;), key = &quot;id&quot;,
                    render = function(item)
                        return ui.Button { id = &quot;entry&quot;, text = item.name, selected = item.selected,
                            on_activate = ui.action(&quot;navigate&quot;, item.id) }
                    end },
            },
            ui.Scroll { id = &quot;apps-scroll&quot;, scroll = ui.bind(&quot;launcher.scroll&quot;), grow = 1,
                ui.Grid { columns = 3, gap = 8,
                    ui.Repeat { id = &quot;apps&quot;, model = ui.bind(&quot;launcher.apps&quot;), key = &quot;id&quot;,
                        render = function(item) return AppTile { item = item } end },
                },
            },
        },
    },
}
</pre></td><td><strong>KDL + Luau — 27 lines</strong><pre>import &quot;bead.ui&quot;
behavior &quot;./behavior.luau&quot;
component &quot;AppTile&quot; {
    property &quot;item&quot; type=&quot;App&quot; required=#true
    button id=&quot;tile&quot; text=(field)&quot;item.name&quot; selected=(field)&quot;item.selected&quot; \
        action=&quot;select&quot; argument=(field)&quot;item.id&quot;
}
panel id=&quot;launcher&quot; width=800 height=480 padding=12 {
    column gap=8 {
        text-input id=&quot;search&quot; text=(bind)&quot;launcher.query&quot; placeholder=&quot;Search applications&quot;
        row gap=12 grow=1 {
            column width=&quot;20%&quot; gap=4 {
                repeat id=&quot;nav&quot; model=(bind)&quot;launcher.navigation&quot; key=&quot;id&quot; {
                    button id=&quot;entry&quot; text=(field)&quot;item.name&quot; selected=(field)&quot;item.selected&quot; \
                        action=&quot;navigate&quot; argument=(field)&quot;item.id&quot;
                }
            }
            scroll id=&quot;apps-scroll&quot; scroll=(bind)&quot;launcher.scroll&quot; grow=1 {
                grid columns=3 gap=8 {
                    repeat id=&quot;apps&quot; model=(bind)&quot;launcher.apps&quot; key=&quot;id&quot; {
                        AppTile item=(row)&quot;item&quot;
                    }
                }
            }
        }
    }
}
</pre></td><td><strong>QML-like proposal — 34 lines</strong><pre>import &quot;bead.ui&quot;
behavior &quot;./behavior.luau&quot;
component AppTile(item: App) {
    Button {
        id: &quot;tile&quot;; text: item.name; selected: item.selected
        onActivate: action(&quot;select&quot;, item.id)
    }
}
Panel {
    id: &quot;launcher&quot;; width: 800; height: 480; padding: 12
    Column {
        gap: 8
        TextInput { id: &quot;search&quot;; text: bind(&quot;launcher.query&quot;); placeholder: &quot;Search applications&quot; }
        Row {
            gap: 12; grow: 1
            Column {
                width: &quot;20%&quot;; gap: 4
                Repeat {
                    id: &quot;nav&quot;; model: bind(&quot;launcher.navigation&quot;); key: &quot;id&quot;
                    Button { id: &quot;entry&quot;; text: item.name; selected: item.selected
                        onActivate: action(&quot;navigate&quot;, item.id) }
                }
            }
            Scroll {
                id: &quot;apps-scroll&quot;; scroll: bind(&quot;launcher.scroll&quot;); grow: 1
                Grid {
                    columns: 3; gap: 8
                    Repeat { id: &quot;apps&quot;; model: bind(&quot;launcher.apps&quot;); key: &quot;id&quot;
                        AppTile { item: item } }
                }
            }
        }
    }
}
</pre></td></tr></tbody></table>

Files: [Luau](../../spikes/authoring/panel.luau), [KDL](../../spikes/authoring/panel.kdl), [QML-like](../../spikes/authoring/panel.bqml).

## Comparison

| Criterion | Luau markup | KDL + Luau | QML-like |
| --- | --- | --- | --- |
| Agents/humans | Familiar table/function syntax; one language for components/behavior. Nested punctuation is the cost. | Clear tree; compact, but typed binding/field tags and component conventions are Bead-specific. | Familiar to existing QML agents; risks invented unsupported Qt/JS features. |
| File/line errors | Existing compiler/runtime supplies lines; Rust constructors capture calling file/line and name bad property. Individual property column needs extra work. | Existing parser has byte spans for nodes/entries; best precise structure/property locations. Behavior still Luau. | We own lexer/parser/spans/error recovery plus a formatter; none measured/implemented yet. |
| Typing | Rust validates node/layout/action/binding/model schemas. Luau annotations/editor checker possible; mlua does not run static checking. | Rust schema checks + annotated values; KDL itself does not know widget or binding types. | Custom property/scope/type checker still needed; surface resemblance to QML is not Qt typing. |
| Hot reload | Execute candidate pure builders in bounded sandbox, lower to typed views, discard temporary VM; all staging/state stays in Rust. | Parse candidate tree, lower same view IR; same Rust transaction, plus behavior VM. | New parser lowers same IR; same transaction once built. |
| License | mlua0.12.2 + bundled Luau740: MIT. Already allowed/used. | kdl6.7.1: Apache-2.0; spans enabled by default. Plus MIT Luau. | Own code MIT/Apache; permissive parser tools possible. No Qt/GPL parser code in Bead. |
| Verified tonight | Luau compiler accepted both files; annotations not statically checked. | Actual kdl6.7.1 parser accepted KDL v2 source. | Hand-reviewed syntax proposal only; no fabricated parse/type/performance results. |

## Decision and boundary

Choose **Luau-as-markup**. Four fewer KDL lines do not justify a second set of
binding/component conventions. There is no demonstrated Luau blocker for this
panel. A new QML-like language has the largest maintenance cost and an unmeasured
parser/type/tooling burden. This judgment is engineering judgment, not a user study.

`FrontEnd` loads tracked sources and returns a typed `ViewSpec` tree with source
locations, stable local IDs, layout values, bindings, named actions and properties.
Only this boundary knows Luau/KDL/custom syntax. Rust owns stores, models, keyed
identity, Taffy layout, reload commit, renderer and services. Replacing the front
end must not replace those systems. Current JSON remains a legacy test path.

Ordinary pure Luau functions are components; jailed tracked `require` imports
provide reuse. Markup constructs nodes; business logic stays in Rust/behavior
modules. No UI-side processes, service subscriptions or mutation of live stores.
Bindings are explicit handles; a plain snapshot string is not a reactive binding.
Repeat identities use a stable repeater ID plus the model key, never list index.
Type/action/reference/geometry errors reject the whole candidate. Required
properties and unknown-property checks prevent silently ignored typos.

Keep the SDK bounded to rows/columns/grids/scroll, controls/text/images, style values,
repeaters, components/imports, bindings and named actions; no own layout/text engine.
Property diagnostics initially report constructor line and property name, not a
fictional exact token span. `--!strict` is not a guarantee of runtime static checking:
[Luau annotations are compile-time tooling](https://luau.org/types/) and mlua is VM/compiler integration.
The separate upstream `Luau.Analysis` / `luau-analyze` integration remains explicit
future tooling if needed, not silently described as delivered.

## Checks and sources

`cargo run --manifest-path spikes/authoring/Cargo.toml` reproduces syntax counts.
Checked2026-10-13: [mlua0.12.2](https://docs.rs/mlua/0.12.2/mlua/),
[Luau syntax](https://luau.org/syntax/), [MIT license](https://github.com/luau-lang/luau/blob/master/LICENSE.txt),
[kdl6.7.1 source/license/spans](https://docs.rs/crate/kdl/6.7.1/source/Cargo.toml).
mlua0.12.2 published2026-10-03; kdl6.7.1 published2026-05-31 (crates.io metadata).
Existing lock uses luau0-src0.22.0+luau740. Taffy0.15.0 published2026-10-12 (MIT)
is the layout candidate; validate APIs when implementing, rather than assuming old versions.
No new renderer/GPU memory or latency measurement applies to this syntax-only step.

Next: implement the typed frontend boundary + Taffy/keyed lists/imports, then
real launcher sidebar/grid and100-cycle/state mutation proofs. No owner wait tonight.
Baking off-thread and shader-cache startup work are deferred by owner instruction.
