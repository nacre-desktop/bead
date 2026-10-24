-- SPDX-License-Identifier: GPL-3.0-only
-- QML NacreStatusPeek notification summary, projected by ControlCenter.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local count=ui.resolve_for_check(ui.bind("cc.notice_count"))
local title_height,summary_height,gap=18,20,6
local height=title_height+gap+summary_height+30
return kit.Panel{id="notification-peek",edge="top",open=ui.bind("panel.open"),duration=0.23,transition="ease-in-out",
    fade=false,reveal=true,width=310,height=height,padding=15,gap=gap,
    radius=T.radii.medium,fill="surfaceContainerHigh",
    ui.Text{id="heading",text=ui.tr("bead.panel.control-notifications"),width=280,height=title_height,
        font_size=T.text.caption*4/3,font_weight=500,color="onSurfaceVariant"},
    ui.Text{id="summary",text=ui.tr("bead.notifications.count",{count=count}),width=280,height=summary_height,
        font_size=T.text.body*4/3,color="onSurface"},
}
