-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local measure=require("../measure")
local status_text=c.status_text
local popouts=c.popouts
local status_glyph=c.status_glyph
local font="IBM Plex Sans"
local count_max=ui.text_width("999…",font,t.detail_font)
local function count_reservation()
    return t.status_padding+t.status_count_padding+t.status_ink.notifications+t.item_gap+count_max
end
local function bar(props,settings,_data)
    local style=props.form_style
    local available=props.available_width and math.max(0,props.available_width-t.status_padding-t.status_count_padding
        -(t.status_symbol_ink.notifications or t.status_ink.notifications)-t.item_gap)
    local value_capacity=math.min(count_max,available or count_max)
    local text_capacity=available or ui.text_width("Notifications",font,t.detail_font)
    local content_capacity=style=="icon+value" and value_capacity or text_capacity
    local count=ui.bind("bar.notifications.text")
    local current=ui.resolve_for_check(count) or "0"
    local raw_count=tonumber(ui.resolve_for_check(ui.bind("bar.notifications.count")) or current)
    local bounded=raw_count and raw_count>999 and "999…" or current
    local text=style=="icon+value" and measure.ellipsize(bounded,value_capacity,font,t.detail_font)
        or style=="icon+text" and measure.ellipsize("Notifications",text_capacity,font,t.detail_font) or nil
    local width=text and ui.text_width(text,font,t.detail_font) or 0
    if text then
        width=ui.animate("notifications-width:"..tostring(props.instance and props.instance.iid or "preview"),width,.18,"out-cubic")
        width=math.min(width,style=="icon+value" and value_capacity or text_capacity)
    end
    return (popouts and kit.StatusIcon or ui.Row){id="notifications",width=text and props.available_width or nil,direction="row",height=t.status_height,gap=style~="icon" and t.item_gap or 0,padding=0,padding_left=t.status_padding,padding_right=t.status_count_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify=text and "start" or "center",
        aria_label=ui.tr("bead.bar.open-notifications"),on_activate=popouts and ui.action("status.pill.open","notifications") or nil,
        status_glyph("glyph","notifications","notifications"),
        text and status_text{id="notifications-count",text=text,width=width,max_width=content_capacity,font_size=t.detail_font,color="secondary"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=280,h=80}
    return ui.Box{id="notifications-desktop",width=slot.w,height=slot.h,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.medium,fill="surfaceContainer",on_activate=popouts and ui.action("status.pill.open","notifications") or nil,
        ui.Row{id="content",width="fill",height="fill",gap=kit.tokens.spacing.md,align="center",status_glyph("glyph","notifications","notifications"),
            ui.Column{id="labels",width="fill",gap=kit.tokens.spacing.xs,ui.Text{id="title",text="Notifications",font_size=kit.tokens.text.body*4/3,color="onSurface"},
                ui.Text{id="value",text=ui.bind("bar.notifications.text"),font_size=kit.tokens.text.title*4/3,color="primary"}},
        }}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
local function count_style()
    local descriptor=style("Icon + value",2,bar)
    descriptor.reserve_width=function(_) return count_reservation() end
    return descriptor
end
return {id="notifications",title="Notifications",icon="notifications",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon+value",styles={icon=style("Icon",1,bar),["icon+value"]=count_style(),["icon+text"]=style("Icon + text",3,bar)}},
        desktop={default_style="summary",styles={summary={title="Summary",min_span=1,max_span=1200,preferred_span=320,render=desktop}}},
        panel={styles={notifications={view="notifications"}}}}
}
