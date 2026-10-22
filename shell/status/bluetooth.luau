-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local ready=ui.resolve_for_check(ui.bind("status.ready"))
local active=ui.resolve_for_check(ui.bind("status.active"))
local visible=ui.resolve_for_check(ui.bind("bluetooth.power_visible"))
local enabled=ui.resolve_for_check(ui.bind("bluetooth.power_enabled"))
local padding=kit.tokens.spacing.md
local icon_size=kit.tokens.icons.xlarge*4/3
local width=ui.resolve_for_check(ui.bind("view.width"))
local height=ui.resolve_for_check(ui.bind("view.height"))
local duration=ui.resolve_for_check(ui.bind("view.duration"))
local text_width=width-padding*2-icon_size-kit.tokens.spacing.md
return kit.Panel{id="status-popout",edge=ui.resolve_for_check(ui.bind("view.edge")),
    open=ui.bind("panel.open"),duration=duration,width=width,height=height,padding=padding,
    gap=kit.tokens.spacing.sm,radius=kit.tokens.radii.medium,fill="surface",
    ui.Text{id="heading",text=ui.bind("status.heading"),height=24,font_size=kit.tokens.text.title*4/3,color="onSurface"},
    ui.Row{id="summary",height=88,gap=kit.tokens.spacing.md,align="center",
        ui.Text{id="icon",text=ui.bind("status.icon"),family="Material Symbols Rounded",font_size=icon_size,
            width=icon_size,height=42,shrink=0,color=ready and active and "primary" or "onSurfaceVariant"},
        ui.Column{width=text_width,height=84,gap=2,
            ui.Text{id="primary",text=ui.bind("status.primary"),font_size=kit.tokens.text.body*4/3,height=60,color="onSurface"},
            ui.Text{id="secondary",text=ui.bind("status.secondary"),font_size=kit.tokens.text.caption*4/3,height=22,color="onSurfaceVariant"},
        },
    },
    ui.Text{id="detail",text=ui.bind("status.detail"),font_size=kit.tokens.text.caption*4/3,height=visible and 24 or 60,color="onSurfaceVariant"},
    visible and ui.Row{id="bluetooth-row",height=36,align="center",gap=kit.tokens.spacing.md,
        ui.Text{id="bluetooth-label",text=ui.tr("bead.panel.bluetooth"),height=28,grow=1,font_size=kit.tokens.text.body*4/3,
            color=enabled and "onSurface" or "onSurfaceVariant"},
        kit.Switch{id="bluetooth-power",checked=ui.bind("bluetooth.power_checked"),aria_label=ui.tr("bead.panel.bluetooth-radio"),
            on_activate=enabled and ui.action("bluetooth.toggle_power") or nil},
    } or nil,
    visible and ui.Text{id="bluetooth-message",text=ui.bind("bluetooth.power_message"),font_size=kit.tokens.text.caption*4/3,height=20,color="onSurfaceVariant"} or nil,
}
