-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
return kit.Panel{id="status-peek",edge="top",open=ui.bind("panel.open"),duration=0.23,transition="ease-in-out",
    width=190,height=72,padding=kit.tokens.spacing.sm,gap=kit.tokens.spacing.xs,
    radius=kit.tokens.radii.medium,fill="surfaceContainerHigh",
    ui.Text{id="heading",text=ui.bind("status.heading"),height=20,font_size=kit.tokens.text.caption*4/3,color="onSurfaceVariant"},
    ui.Text{id="primary",text=ui.bind("status.primary"),width=174,height=32,font_size=kit.tokens.text.body*4/3,color="onSurface"},
}
