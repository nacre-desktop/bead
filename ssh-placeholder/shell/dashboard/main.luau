-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local page=ui.resolve_for_check(ui.bind("dashboard.page"))
local preview=ui.resolve_for_check(ui.bind("dashboard.preview"))
local stale=ui.resolve_for_check(ui.bind("dashboard.stale"))
local content=require("./page")
local nav={}
local tabs=ui.resolve_for_check(ui.bind("dashboard.tabs"))
local message=ui.resolve_for_check(ui.bind("dashboard.message"))
local customize=ui.resolve_for_check(ui.bind("dashboard.customize"))
-- Slots share the strip; labels drop out when a slot gets too narrow.
local count=math.max(1,#tabs)
local tw=936/count
local labeled=tw>=94
local selected_slot=nil
for index,tab in ipairs(tabs) do if tab.id==page then selected_slot=index break end end
local moving=ui.resolve_for_check(ui.bind("dashboard.pearl.moving"))
-- The selected pearl follows the caption centre, excluding the icon and its gap.
local caption_offset=labeled and (19+7)/2 or 0
local pearl_left=selected_slot and (moving and ui.bind("dashboard.pearl.x") or (selected_slot-1)*tw+(tw-12)/2+caption_offset) or nil
-- QML NacreDashboardNavigation: icon + title centred at y=6, accent icon / ink title when selected, an 8 px pearl (12 px halo) under the selected slot; no pill.
for _,tab in ipairs(tabs) do
    local title=tab.id=="today" and ui.tr("bead.dashboard.nav-dashboard") or tab.title
    local selected=tab.id==page
    local interactive=not selected
    local Button=interactive and kit.Button or ui.Box
    local label_width=labeled and math.ceil(ui.text_width(title,"IBM Plex Sans",13.333))+kit.tokens.spacing.hairline/2 or 0
    local rw=19+(labeled and 7+label_width or 0)
    local face={
        ui.Row{id="label",position="absolute",left=(tw-rw)/2,top=6,width=rw,height=24,gap=7,align="center",justify="center",
            ui.Text{text=tab.icon,family="Material Symbols Rounded",width=19,height=24,font_size=18.667,color=selected and "primary" or "onSurfaceVariant"},
            labeled and ui.Text{text=title,width=label_width,height=24,font_size=13.333,text_align="center",color=selected and "onSurface" or "onSurfaceVariant"} or nil,
        },
    }
    table.insert(nav,Button{id=tab.id,width=tw,height=44,padding=0,radius=12,
        fill="transparent",hover_fill=interactive and ui.alpha("onSurface",0.06) or nil,
        aria_label=title,on_activate=interactive and ui.action("dashboard.select",tab.id) or nil,
        table.unpack(face)})
end
if pearl_left then
    nav[#nav+1]=ui.Box{id="tab-halo",position="absolute",left=pearl_left,top=29,width=12,height=12,radius=6,fill=ui.alpha("primary",0.12),
        ui.Box{id="tab-pearl",position="absolute",left=2,top=2,width=8,height=8,radius=4,fill="primary"}}
end
return kit.Panel{id="dashboard",edge="top",open=ui.bind("panel.open"),duration=.36,return_duration=.18,return_transition="ease",transition="ease-in-out",fade=false,reveal=not ui.resolve_for_check(ui.bind("view.detached")),
    width=984,height=650,padding=0,radius=18,fill="surface",
    ui.Row{id="navigation",position="absolute",left=24,top=12,width=936,height=44,table.unpack(nav)},
    ui.Box{id="stage",position="absolute",left=22,top=68,width=940,height=562,content()},
    ui.Text{id="state",position="absolute",left=24,top=632,width=936,height=16,font_size=12,text_align="right",
        text=preview and (stale and ui.tr("bead.dashboard.state-preview-stale") or ui.tr("bead.dashboard.state-preview")) or stale and ui.tr("bead.dashboard.state-stale") or "",color=stale and not preview and "error" or "onSurfaceVariant"},
    message~="" and ui.Text{id="message",position="absolute",left=24,top=632,width=600,height=16,font_size=12,text=ui.bind("dashboard.message"),color="error"} or nil,
    customize and message=="" and kit.Button{id="customize",position="absolute",left=24,top=626,width=170,height=24,padding=0,radius=8,fill="transparent",
        aria_label=ui.tr("bead.dashboard.customize-pages"),on_activate=ui.action("view.open","dashboard-customize"),
        ui.Text{id="customize-label",text=ui.tr("bead.dashboard.customize-pages"),width=170,height=16,font_size=12,color="onSurfaceVariant"}} or nil,
}
