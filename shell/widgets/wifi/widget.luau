-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local status_text=c.status_text
local function bar(props,settings,_data)
    local glyph=ui.resolve_for_check(ui.bind("bead.desktop.network")) or "wifi_off"
    local value=glyph=="wifi" and "Connected" or "Offline"
    local style=props.form_style
    local content=style=="icon+value" and value or style=="icon+text" and "Wi-Fi" or nil
    local width=content and ui.text_width(content,"IBM Plex Sans",t.detail_font) or 0
    return (popouts and kit.StatusIcon or ui.Row){id="network",direction="row",height=t.status_height,gap=style~="icon" and t.item_gap or nil,padding=0,padding_left=t.status_padding,padding_right=t.status_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
        aria_label=ui.tr("bead.bar.open-network"),on_activate=popouts and ui.action("status.pill.open","network") or nil,
        status_glyph("glyph",ui.bind("bead.desktop.network"),"network"),
        content and status_text{id="value",text=content,width=width,font_size=t.detail_font,color="secondary"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=280,h=80}
    local glyph=ui.resolve_for_check(ui.bind("bead.desktop.network")) or "wifi_off"
    return ui.Box{id="network-desktop",width=slot.w,height=slot.h,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.medium,fill="surfaceContainer",on_activate=popouts and ui.action("status.pill.open","network") or nil,
        ui.Row{id="content",width="fill",height="fill",gap=kit.tokens.spacing.md,align="center",status_glyph("glyph",ui.bind("bead.desktop.network"),"network"),
            ui.Column{id="labels",width="fill",gap=kit.tokens.spacing.xs,ui.Text{id="title",text="Wi-Fi",font_size=kit.tokens.text.body*4/3,color="onSurface"},
                ui.Text{id="value",text=glyph=="wifi" and "Connected" or "Offline",font_size=kit.tokens.text.title*4/3,color="primary"}},
        }}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="wifi",title="Wi-Fi",icon="wifi",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="status",styles={status={title="Status",min_span=1,max_span=1200,preferred_span=300,render=desktop}}},
        panel={styles={network={view="network"}}}}
}
