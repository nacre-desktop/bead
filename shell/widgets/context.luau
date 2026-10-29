-- SPDX-License-Identifier: GPL-3.0-only
-- Loading this module reads no stores (B0196), so widget modules that require it stay loaded
-- across builds. Store values are functions: call them from render functions, not at load.
local base_ui=require("bead.ui")
local ui={}
for key,value in base_ui do ui[key]=value end
-- Widget previews substitute sample values for stores. Each entry calls begin() once per build, so
-- this reads widgets.preview once per build instead of once per bind.
local NIL={}
local sample
local reads={}
local function begin()
    table.clear(reads)
    local samples=base_ui.resolve_for_check(base_ui.bind("widgets.preview"))
    sample=samples and samples[1]
end
ui.bind=function(key)
    if sample then
        local value=sample[key]
        if type(value)=="table" and value.fallback then
            local binding=base_ui.bind(key)
            local current=base_ui.resolve_for_check(binding)
            return current~=nil and current~="" and binding or value.fallback
        end
        return value~=nil and value or base_ui.bind(key)
    end
    return base_ui.bind(key)
end
local kit=require("bead.kit")
local t=require("../bar/tokens")
local function centered_text(p)
    local lift=p.family=="Material Symbols Rounded" and 0 or t.text_lift
    p.height=p.font_size*t.line_height+2*lift
    if p.position=="absolute" then p.top=(p.top or 0)-lift end
    return ui.Text(p)
end
-- Values are read once per build (stores cannot change within one) and shared by every widget.
local function read(key)
    return function()
        local value=reads[key]
        if value==nil then
            value=ui.resolve_for_check(ui.bind(key))
            reads[key]=value==nil and NIL or value
        elseif value==NIL then value=nil end
        return value
    end
end
local launcher=read("bar.launcher")
local popouts=read("bar.popouts")
local ws_on=read("bar.workspace.enabled")
local tray_on=read("bar.tray.enabled")
local dash_on=read("bar.dashboard.enabled")
local focus_on=read("bar.window.focus.enabled")
local updates_on=read("bar.updates.enabled")
local updates_n=read("bar.updates.count")
local header=read("bar.height")
local function title() return ui.bind("bar.title") end
local function title_width()
    return math.min(t.title_max_width,math.ceil(ui.text_width(title(),"IBM Plex Sans",t.title_font))+kit.tokens.spacing.hairline/2)
end
local function cue_extent() return math.min(t.title_cue_max,title_width()+t.icon_width+t.title_gap+10) end
local dwell=read("bead.edge.dwell")
local ws_count=read("bar.workspace.count")
local ws_icons=read("bar.workspace.icons")
local function thread()
    for _,entry in ws_icons() do if entry.icon~="" then return false end end
    return true
end
-- QML NacreWorkspaceRow.tint: pearls blend primary -> secondary -> tertiary by position along the row.
local function pearl_accent(at)
    local position=math.min(1,at/math.max(1,ws_count()-1))
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
return {begin=begin,ui=ui,kit=kit,t=t,centered_text=centered_text,launcher=launcher,popouts=popouts,ws_on=ws_on,tray_on=tray_on,dash_on=dash_on,focus_on=focus_on,updates_on=updates_on,updates_n=updates_n,header=header,title=title,title_width=title_width,cue_extent=cue_extent,dwell=dwell,ws_count=ws_count,ws_icons=ws_icons,thread=thread,pearl_accent=pearl_accent,glyph=glyph,status_glyph=status_glyph,status_text=status_text}
