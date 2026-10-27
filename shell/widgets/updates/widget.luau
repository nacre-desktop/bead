-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local measure=require("../measure")
local status_text=c.status_text
local popouts=c.popouts
local updates_on=c.updates_on
local status_glyph=c.status_glyph
local font="IBM Plex Sans"
local count_max=ui.text_width("999…",font,t.clock_font)
local function fit_count(value,width)
    return measure.ellipsize(value,math.min(count_max,width or count_max),font,t.clock_font)
end
local function count_reservation()
    return t.status_padding+t.status_count_padding+t.status_ink.package+t.item_gap+count_max
end
local function bar(props,settings,_data)
    local style=props.form_style
    local available=props.available_width and math.max(0,props.available_width-t.status_padding-t.status_count_padding
        -(t.status_symbol_ink.package_2 or t.status_ink.package)-t.item_gap)
    local count_capacity=math.min(count_max,available or count_max)
    local text_capacity=available or ui.text_width("Updates",font,t.clock_font)
    local content_capacity=style=="icon+value" and count_capacity or text_capacity
    local raw_count=ui.resolve_for_check(ui.bind("bar.updates.text")) or "0"
    local count=tonumber(raw_count)
    local bounded=count and count>999 and "999…" or tostring(raw_count)
    local content=style=="icon+value" and fit_count(bounded,count_capacity)
        or style=="icon+text" and "Updates" or nil
    if content and style=="icon+text" then content=measure.ellipsize(content,text_capacity,font,t.clock_font) end
    local width=content and ui.text_width(content,font,t.clock_font) or 0
    if content then
        width=ui.animate("updates-width:"..tostring(props.instance and props.instance.iid or "preview"),width,.18,"out-cubic")
        width=math.min(width,content_capacity)
    end
    local action=popouts and ui.action("status.pill.open","updates") or nil
    return updates_on and kit.Button{id="updates",width=content and props.available_width or nil,direction="row",padding=0,fill="transparent",hover_fill="surfaceContainerHigh",on_activate=action,height=t.status_height,gap=t.item_gap,shrink=0,padding_left=t.status_padding,padding_right=t.status_count_padding,align="center",justify=content and "start" or "center",
        status_glyph("package","package_2","package","primary"),
        content and status_text{id="count",text=content,width=width,max_width=content_capacity,font_size=t.clock_font,color="primary"} or nil,
    } or ui.Box{id="updates",width=0,height=0,shrink=0,visible=false}
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=280,h=80}
    local width=slot.w
    local height=slot.h
    local style=props.form_style or "card"
    local count=ui.resolve_for_check(ui.bind("bar.updates.text")) or "0"
    local checked=ui.bind("status.updates_checked")
    local open=ui.resolve_for_check(ui.bind("status.updates_can_open"))==true
    local action=popouts and ui.action("status.pill.open","updates") or nil
    if style=="wallpaper" then
        return ui.Column{id="updates-wallpaper",width=width,height=height,fill="transparent",justify="center",gap=kit.tokens.spacing.xs,
            ui.Text{id="count",text=count,width="fill",height=72,font_size=kit.tokens.text.headline*2,color="primary"},
            ui.Text{id="checked",text=checked,width="fill",height=24,font_size=kit.tokens.text.body*4/3,color="onSurface"},
            kit.Button{id="update",text="Update",height=38,enabled=open,focus_on_pointer=false,on_activate=open and ui.action("updates.open") or nil}}
    end
    return ui.Box{id="updates-desktop",width=width,height=height,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.large,
        fill=style=="pearl" and "surfaceContainerHigh" or "surfaceContainer",on_activate=action,
        ui.Column{id="updates-card",width="fill",height="fill",gap=kit.tokens.spacing.sm,
            ui.Row{id="header",width="fill",height=28,gap=kit.tokens.spacing.sm,align="center",
                status_glyph("package","package_2","package","primary"),
                ui.Text{id="title",text="Updates",width=0,grow=1,font_size=kit.tokens.text.title*4/3,color="onSurface"},
                ui.Text{id="count",text=count,font_size=kit.tokens.text.headline*4/3,color="primary"}},
            ui.Text{id="checked",text=checked,width="fill",height=24,font_size=kit.tokens.text.caption*4/3,color="onSurfaceVariant"},
            ui.Row{id="actions",width="fill",height=38,gap=kit.tokens.spacing.xs,align="center",
                kit.Button{id="update",text="Update",height=36,enabled=open,focus_on_pointer=false,on_activate=open and ui.action("updates.open") or nil},
                kit.Button{id="check",text="Check",height=36,focus_on_pointer=false,on_activate=ui.action("updates.refresh")}}}}
end
local function style(title,preferred,render)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render}
end
local function count_style(title,preferred)
    local descriptor=style(title,preferred,bar)
    descriptor.reserve_width=function(_) return count_reservation() end
    return descriptor
end
return {id="updates",title="Updates",icon="package_2",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon+value",styles={
        icon=style("Icon",1,bar),["icon+value"]=count_style("Icon + value",2),["icon+text"]=style("Icon + text",3,bar),
    }},desktop={default_style="card",styles={
        summary={title="Stored summary",min_span=1,max_span=1200,preferred_span=280,preferred_height=80,hidden=true,render=function(p,s,d) p.form_style="card"; return desktop(p,s,d) end},
        card={title="Update card",min_span=1,max_span=1200,preferred_span=320,preferred_height=160,render=function(p,s,d) p.form_style="card"; return desktop(p,s,d) end},
        pearl={title="Pearl notice",min_span=1,max_span=1200,preferred_span=320,preferred_height=160,render=function(p,s,d) p.form_style="pearl"; return desktop(p,s,d) end},
        wallpaper={title="Wallpaper count",min_span=1,max_span=1200,preferred_span=280,preferred_height=150,render=function(p,s,d) p.form_style="wallpaper"; return desktop(p,s,d) end}}},
        panel={styles={updates={view="updates"}}}}
}
