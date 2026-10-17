-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local ready=ui.resolve_for_check(ui.bind("status.ready"))
local active=ui.resolve_for_check(ui.bind("status.active"))
local battery=ui.resolve_for_check(ui.bind("status.kind"))=="battery"
local padding=kit.tokens.spacing.md
local icon_size=kit.tokens.icons.xlarge*4/3
local text_width=310-padding*2-icon_size-kit.tokens.spacing.md
return kit.Panel{id="status-popout",edge=ui.resolve_for_check(ui.bind("view.edge")),
    open=ui.bind("panel.open"),duration=kit.motion.color,width=310,height=222,padding=padding,
    gap=kit.tokens.spacing.sm,radius=kit.tokens.radii.medium,fill="surface",
    ui.Text{id="heading",text=ui.bind("status.heading"),height=24,font_size=kit.tokens.text.title*4/3,color="onSurface"},
    ui.Row{id="summary",height=88,gap=kit.tokens.spacing.md,align="center",
        ui.Text{id="icon",text=ui.bind("status.icon"),family="Material Symbols Rounded",font_size=icon_size,
            width=icon_size,height=42,shrink=0,color=ready and active and "primary" or "onSurfaceVariant"},
        ui.Column{width=text_width,height=84,gap=2,
            ui.Text{id="primary",text=ui.bind("status.primary"),font_size=(battery and ready and kit.tokens.text.headline or kit.tokens.text.body)*4/3,height=60,color="onSurface"},
            ui.Text{id="secondary",text=ui.bind("status.secondary"),font_size=kit.tokens.text.caption*4/3,height=22,color="onSurfaceVariant"},
        },
    },
    ui.Text{id="detail",text=ui.bind("status.detail"),font_size=kit.tokens.text.caption*4/3,height=60,color="onSurfaceVariant"},
}
