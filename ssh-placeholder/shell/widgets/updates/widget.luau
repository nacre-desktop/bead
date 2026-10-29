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
local visual=require("../desktop-visuals")
local font="IBM Plex Sans"
local count_max=ui.text_width("999…",font,t.clock_font)
local function fit_count(value,width)
    return measure.ellipsize(value,math.min(count_max,width or count_max),font,t.clock_font)
end
local function count_reservation()
    local hit_pad=popouts and t.status_hit_pad or 0
    return t.status_padding+t.status_count_padding+2*hit_pad+t.status_ink.package+t.item_gap+count_max
end
local function bar(props,settings,_data)
    local hit_pad=popouts and t.status_hit_pad or 0
    local style=props.form_style
    local available=props.available_width and math.max(0,props.available_width-t.status_padding-t.status_count_padding-2*hit_pad
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
    return updates_on and kit.Button{id="updates",width=content and props.available_width or nil,direction="row",padding=0,fill="transparent",hover_fill="surfaceContainerHigh",on_activate=action,height=t.status_height,radius=t.status_radius,gap=t.item_gap,shrink=0,padding_left=t.status_padding+hit_pad,padding_right=t.status_count_padding+hit_pad,align="center",justify=content and "start" or "center",
        status_glyph("package","package_2","package","primary"),
        content and status_text{id="count",text=content,width=width,max_width=content_capacity,font_size=t.clock_font,color="primary"} or nil,
    } or ui.Box{id="updates",width=0,height=0,shrink=0,visible=false}
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=320,h=160}
    local width=slot.w
    local height=slot.h
    local count=ui.resolve_for_check(ui.bind("bar.updates.text")) or ""
    local checked=ui.bind("status.updates_checked")
    local open=ui.resolve_for_check(ui.bind("status.updates_can_open"))==true
    return visual.glass("updates-desktop",width,height,kit.tokens.spacing.lg,{
        ui.Column{id="updates-card",width="fill",height="fill",justify="center",gap=kit.tokens.spacing.sm,
            ui.Text{id="title",text="System updates",width="fill",height=24,font_size=kit.tokens.text.body*4/3,color="onSurfaceVariant"},
            ui.Row{id="count-row",width="fill",height=70,align="center",gap=kit.tokens.spacing.sm,
                visual.display("count",count,112,70,64),
                count~="" and ui.Text{id="packages-label",text="packages",width=0,grow=1,height=24,font_size=kit.tokens.text.body*4/3,color="onSurface"} or nil},
            ui.Text{id="checked",text=checked,width="fill",height=20,font_size=kit.tokens.text.caption*4/3,color=visual.secondary},
            kit.Button{id="update",text="Update now",height=42,padding_left=kit.tokens.spacing.lg,padding_right=kit.tokens.spacing.lg,
                fill="primary",color="onPrimary",radius=21,focus_on_pointer=false,on_activate=open and ui.action("updates.open") or nil}}
    })
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
        summary={title="Legacy update summary",min_span=1,max_span=1200,preferred_span=280,preferred_height=80,hidden=true,render=desktop},
        card={title="Update card",min_span=1,max_span=1200,preferred_span=320,preferred_height=160,render=desktop},
        pearl={title="Legacy update panel",min_span=1,max_span=1200,preferred_span=320,preferred_height=160,hidden=true,render=desktop},
        wallpaper={title="Legacy wallpaper count",min_span=1,max_span=1200,preferred_span=280,preferred_height=150,hidden=true,render=desktop}}},
        panel={styles={updates={view="updates"}}}}
}
