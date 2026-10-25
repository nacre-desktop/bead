-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local status_text=c.status_text
local function bar(props,settings,_data)
    local glyph=ui.resolve_for_check(ui.bind("bead.desktop.bluetooth")) or "bluetooth_disabled"
    local value=glyph=="bluetooth" and "On" or "Off"
    local style=props.form_style
    local content=style=="icon+value" and value or style=="icon+text" and "Bluetooth" or nil
    local width=content and ui.text_width(content,"IBM Plex Sans",t.detail_font) or 0
    return (popouts and kit.StatusIcon or ui.Row){id="bluetooth",height=t.status_height,gap=style~="icon" and t.item_gap or nil,padding=0,padding_left=t.status_padding,padding_right=t.status_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
        aria_label=ui.tr("bead.bar.open-bluetooth"),on_activate=popouts and ui.action("status.pill.open","bluetooth") or nil,
        status_glyph("glyph",ui.bind("bead.desktop.bluetooth"),"bluetooth"),
        content and status_text{id="value",text=content,width=width,font_size=t.detail_font,color="secondary"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=280,h=80}
    local glyph=ui.resolve_for_check(ui.bind("bead.desktop.bluetooth")) or "bluetooth_disabled"
    return ui.Box{id="bluetooth-desktop",width=slot.w,height=slot.h,padding=12,radius=kit.tokens.radii.medium,fill="surfaceContainer",on_activate=popouts and ui.action("status.pill.open","bluetooth") or nil,
        ui.Row{id="content",width="fill",height="fill",gap=12,align="center",status_glyph("glyph",ui.bind("bead.desktop.bluetooth"),"bluetooth"),
            ui.Column{id="labels",width="fill",gap=4,ui.Text{id="title",text="Bluetooth",font_size=16,color="onSurface"},
                ui.Text{id="value",text=glyph=="bluetooth" and "On" or "Off",font_size=20,color="primary"}},
        }}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="bluetooth",title="Bluetooth",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",3,bar)}},
        desktop={default_style="status",styles={status={title="Status",min_span=1,max_span=1200,preferred_span=300,render=desktop}}},
        panel={styles={bluetooth={view="bluetooth"}}}}
}
