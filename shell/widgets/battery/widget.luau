-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local status_text=c.status_text
local popouts=c.popouts
local status_glyph=c.status_glyph
local visual=require("../desktop-visuals")
local function bar(props,settings,_data)
    local style=props.form_style
    local value=ui.bind("bead.desktop.battery.value")
    local content=style=="icon+value" and value or style=="icon+text" and "Battery" or nil
    local current=ui.resolve_for_check(value) or ""
    local width=content and ui.text_width(style=="icon+value" and current or content,"IBM Plex Sans",t.detail_font) or 0
    return (popouts and kit.StatusIcon or ui.Row){id="battery",direction="row",height=t.status_height,gap=style~="icon" and t.item_gap or 0,padding=0,padding_left=t.status_padding,padding_right=t.status_count_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
        aria_label=ui.tr("bead.bar.open-battery"),on_activate=popouts and ui.action("status.pill.open","battery") or nil,
        status_glyph("glyph",ui.bind("bead.desktop.battery"),"battery"),
        content and status_text{id="battery-value",text=content,width=width,font_size=t.detail_font,color="secondary"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local style=props.form_style or "ring"
    local value=ui.resolve_for_check(ui.bind("bead.desktop.battery.value")) or "0%"
    local percent=tonumber(string.match(tostring(value),"%d+")) or 0
    local w,h=visual.slot(props.instance,320,190)
    local gauge=visual.ring("battery-gauge",math.min(h,280),percent,"",nil)
    if style=="frameless" then
        local reading=tostring(percent)
        local reading_width=ui.text_width(reading,visual.display_font,96)+kit.tokens.spacing.lg*4
        return ui.Column{id="battery-line",width=w,height=h,fill="transparent",justify="center",gap=kit.tokens.spacing.sm,
            ui.Row{id="battery-reading",width="fill",height=104,align="center",gap=kit.tokens.spacing.sm,
                visual.display("value",reading,reading_width,104,96,"onSurface",true),
                visual.shadow_text("suffix","% battery","fill",34,kit.tokens.text.body*4/3,"onSurface")},
            ui.Box{id="battery-track",width=300,height=4,fill=ui.alpha("onSurface",.25),radius=2,
                ui.Box{id="battery-fill",position="absolute",left=0,top=0,width=300*percent/100,height=4,fill="primary",radius=2}}}
    end
    if style=="card" then
        return visual.glass("battery-desktop",w,h,kit.tokens.spacing.lg,{
            ui.Row{id="battery-heading",width="fill",height=24,align="center",justify="space-between",
                ui.Text{id="title",text="Battery",font_size=kit.tokens.text.body*4/3,color="secondary"},
                ui.Text{id="charging-note",text="",font_size=kit.tokens.text.caption*4/3,color="primary"}},
            visual.display("value",value,"fill",96,72,"onSurface"),
            ui.Box{id="battery-track",width="fill",height=12,fill=ui.alpha("onSurface",.12),radius=6,
                ui.Box{id="battery-fill",position="absolute",left=0,top=0,width=(w-52)*percent/100,height=12,fill="primary",radius=6}},
            ui.Text{id="charging-note",text="",width="fill",height=20,font_size=kit.tokens.text.caption*4/3,color="secondary"}})
    end
    return ui.Column{id="battery-ring-layout",width=w,height=h,fill="transparent",align="center",justify="center",
            ui.Box{id="battery-ring",width=math.min(h,280),height=math.min(h,280),fill="transparent",gauge}}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="battery",title="Battery",icon="battery_full",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon+value",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="ring",styles={status={title="Stored status",min_span=1,max_span=1200,preferred_span=300,preferred_height=120,hidden=true,render=function(p,s,d) p.form_style="ring"; return desktop(p,s,d) end},
            ring={title="A · Big ring",min_span=1,max_span=1200,preferred_span=320,preferred_height=300,render=function(props,s,d) props.form_style="ring"; return desktop(props,s,d) end},
            card={title="B · Glass card",min_span=1,max_span=1200,preferred_span=360,preferred_height=220,render=function(props,s,d) props.form_style="card"; return desktop(props,s,d) end},
            pearl={title="Legacy gauge alias",min_span=1,max_span=1200,preferred_span=320,preferred_height=220,hidden=true,render=function(props,s,d) props.form_style="card"; return desktop(props,s,d) end},
            frameless={title="C · Minimal line",min_span=1,max_span=1200,preferred_span=420,preferred_height=190,render=function(props,s,d) props.form_style="frameless"; return desktop(props,s,d) end}}},
        panel={styles={battery={view="battery"}}}}
}
