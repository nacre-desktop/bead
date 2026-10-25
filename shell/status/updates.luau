-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local width=ui.resolve_for_check(ui.bind("view.width"))
local height=ui.resolve_for_check(ui.bind("view.height"))
local spacing=kit.tokens.spacing.md
local padding=kit.tokens.spacing.md
return kit.Panel{id="updates-popout",edge="top",open=ui.bind("panel.open"),duration=ui.bind("view.duration"),
    fade=false,reveal=true,width=width,height=height,padding=padding,gap=spacing,
    radius=kit.tokens.radii.medium,fill="surface",
    ui.Text{id="heading",text=ui.bind("status.heading"),height=24,font_size=kit.tokens.text.title*4/3,color="onSurface"},
    ui.Text{id="summary",text=ui.bind("status.updates_summary"),height=28,font_size=kit.tokens.text.body*4/3,color="onSurface"},
    ui.Text{id="detail",text=ui.bind("status.updates_detail"),height=24,font_size=kit.tokens.text.caption*4/3,color="onSurfaceVariant"},
    ui.Row{id="actions",height=38,gap=kit.tokens.spacing.sm,
        kit.Button{id="refresh",text=ui.tr("bead.panel.updates-check"),on_activate=ui.action("updates.refresh"),width=(width-padding*2-spacing)/2,height=38},
        kit.Button{id="open",text=ui.tr("bead.panel.updates-open"),on_activate=ui.action("updates.open"),width=(width-padding*2-spacing)/2,height=38},
    },
}
