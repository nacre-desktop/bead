-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local Calendar=require("./calendar")
local edge=ui.resolve_for_check(ui.bind("view.edge"))
local ready=ui.resolve_for_check(ui.bind("calendar.ready"))
return kit.Panel{id="popout",edge=edge,open=ui.bind("panel.open"),duration=.23,transition="ease-in-out",fade=false,reveal=true,
    width=310,height=312,padding=15,radius=kit.tokens.radii.medium,fill="surface",
    ready and Calendar() or ui.Text{id="unavailable",text=ui.tr("bead.panel.calendar-unavailable"),
        width=280,height=40,font_size=kit.tokens.text.body*4/3,color="onSurfaceVariant"},
}
