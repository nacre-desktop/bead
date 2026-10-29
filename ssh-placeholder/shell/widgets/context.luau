-- SPDX-License-Identifier: GPL-3.0-only
local base_ui=require("bead.ui")
local ui=base_ui
local samples=base_ui.resolve_for_check(base_ui.bind("widgets.preview"))
local sample=samples and samples[1]
if sample then
    ui={}
    for key,value in base_ui do ui[key]=value end
    ui.bind=function(key)
        local value=sample[key]
        if type(value)=="table" and value.fallback then
            local binding=base_ui.bind(key)
            local current=base_ui.resolve_for_check(binding)
            return current~=nil and current~="" and binding or value.fallback
        end
        return value~=nil and value or base_ui.bind(key)
    end
end
local kit=require("bead.kit")
local t=require("../bar/tokens")
local function centered_text(p)
    local lift=p.family=="Material Symbols Rounded" and 0 or t.text_lift
    p.height=p.font_size*t.line_height+2*lift
    if p.position=="absolute" then p.top=(p.top or 0)-lift end
    return ui.Text(p)
end
local launcher=ui.resolve_for_check(ui.bind("bar.launcher"))
local popouts=ui.resolve_for_check(ui.bind("bar.popouts"))
local ws_on=ui.resolve_for_check(ui.bind("bar.workspace.enabled"))
local tray_on=ui.resolve_for_check(ui.bind("bar.tray.enabled"))
local dash_on=ui.resolve_for_check(ui.bind("bar.dashboard.enabled"))
local focus_on=ui.resolve_for_check(ui.bind("bar.window.focus.enabled"))
local updates_on=ui.resolve_for_check(ui.bind("bar.updates.enabled"))
local updates_n=ui.resolve_for_check(ui.bind("bar.updates.count"))
local header=ui.resolve_for_check(ui.bind("bar.height"))
local title=ui.bind("bar.title")
local title_width=math.min(t.title_max_width,math.ceil(ui.text_width(title,"IBM Plex Sans",t.title_font))+kit.tokens.spacing.hairline/2)
local cue_extent=math.min(t.title_cue_max,title_width+t.icon_width+t.title_gap+10)
local dwell=ui.resolve_for_check(ui.bind("bead.edge.dwell"))
local ws_count=ui.resolve_for_check(ui.bind("bar.workspace.count"))
local ws_icons=ui.resolve_for_check(ui.bind("bar.workspace.icons"))
local thread=true
for _,entry in ws_icons do if entry.icon~="" then thread=false end end
-- QML NacreWorkspaceRow.tint: pearls blend primary -> secondary -> tertiary by position along the row.
local function pearl_accent(at)
    local position=math.min(1,at/math.max(1,ws_count-1))
    if position<=0.5 then return ui.tint("primary","secondary",position*2) end
    return ui.tint("secondary","tertiary",(position-0.5)*2)
end
local function glyph(id,text,color)
    return centered_text{id=id,text=text,width=t.icon_width,shrink=0,family="Material Symbols Rounded",font_size=t.icon_font,color=color or "secondary",text_align="center"}
end
local function status_glyph(id,text,kind,color)
    local width=t.status_symbol_ink[ui.resolve_for_check(text)] or t.status_ink[kind]
    return ui.Box{id=id,width=width,height=t.icon_font*t.line_height,shrink=0,
        ui.Text{id="ink",position="absolute",left=(width-t.icon_width)/2,top=0,width=t.icon_width,height=t.icon_font*t.line_height,
            text=text,family="Material Symbols Rounded",font_size=t.icon_font,color=color or "secondary",text_align="center"},
    }
end
local function status_text(p)
    -- Text paints from the top of its line box; centering a shorter box lowers
    -- the baseline without changing the group's hit area or the icon's centre.
    local offset=p.font_size==t.clock_font and t.clock_text_offset or t.status_text_offset
    p.height=p.font_size*t.line_height-2*offset
    p.family="IBM Plex Sans"
    return ui.Text(p)
end
return {ui=ui,kit=kit,t=t,centered_text=centered_text,launcher=launcher,popouts=popouts,ws_on=ws_on,tray_on=tray_on,dash_on=dash_on,focus_on=focus_on,updates_on=updates_on,updates_n=updates_n,header=header,title=title,title_width=title_width,cue_extent=cue_extent,dwell=dwell,ws_count=ws_count,ws_icons=ws_icons,thread=thread,pearl_accent=pearl_accent,glyph=glyph,status_glyph=status_glyph,status_text=status_text}
