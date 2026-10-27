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
    local gauge=visual.ring("battery-gauge",math.min(h-24,136),percent,"Battery",nil)
    if style=="frameless" then
        return ui.Row{id="battery-wallpaper",width=w,height=h,fill="transparent",align="center",gap=kit.tokens.spacing.lg,
            gauge,ui.Column{id="battery-copy",width="fill",height="fill",justify="center",gap=kit.tokens.spacing.xs,
                ui.Text{id="value",text=value,width="fill",height=66,font_size=kit.tokens.text.headline*2,color="onSurface"},
                ui.Text{id="remaining",text="Power remaining",width="fill",height=24,font_size=kit.tokens.text.body*4/3,color="onSurfaceVariant"}}}
    end
    return ui.Box{id="battery-desktop",width=w,height=h,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.large,
        fill=style=="pearl" and "surfaceContainerHigh" or "surfaceContainer",on_activate=popouts and ui.action("status.pill.open","battery") or nil,
        ui.Row{id="content",width="fill",height="fill",gap=kit.tokens.spacing.md,align="center",gauge,
            ui.Column{id="labels",width="fill",justify="center",gap=kit.tokens.spacing.sm,
                ui.Text{id="title",text="Battery",font_size=kit.tokens.text.title*4/3,color="onSurface"},
                ui.Text{id="value",text=value,font_size=kit.tokens.text.headline*4/3,color="primary"},
                ui.Text{id="remaining",text="Time remaining",font_size=kit.tokens.text.body*4/3,color="onSurfaceVariant"}}}}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="battery",title="Battery",icon="battery_full",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon+value",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="ring",styles={status={title="Stored status",min_span=1,max_span=1200,preferred_span=300,preferred_height=120,hidden=true,render=function(p,s,d) p.form_style="ring"; return desktop(p,s,d) end},
            ring={title="Orbit ring",min_span=1,max_span=1200,preferred_span=320,preferred_height=190,render=function(props,s,d) props.form_style="ring"; return desktop(props,s,d) end},
            pearl={title="Pearl gauge",min_span=1,max_span=1200,preferred_span=320,preferred_height=190,render=function(props,s,d) props.form_style="pearl"; return desktop(props,s,d) end},
            frameless={title="Wallpaper type",min_span=1,max_span=1200,preferred_span=420,preferred_height=190,render=function(props,s,d) props.form_style="frameless"; return desktop(props,s,d) end}}},
        panel={styles={battery={view="battery"}}}}
}
