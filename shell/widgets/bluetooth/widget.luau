-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local status_text=c.status_text
local visual=require("../desktop-visuals")
local function bar(props,settings,_data)
    local glyph=ui.resolve_for_check(ui.bind("bead.desktop.bluetooth")) or "bluetooth_disabled"
    local value=glyph=="bluetooth" and "On" or "Off"
    local style=props.form_style
    local content=style=="icon+value" and value or style=="icon+text" and "Bluetooth" or nil
    local width=content and ui.text_width(content,"IBM Plex Sans",t.detail_font) or 0
    return (popouts and kit.StatusIcon or ui.Row){id="bluetooth",direction="row",height=t.status_height,gap=style~="icon" and t.item_gap or nil,padding=0,padding_left=t.status_padding,padding_right=t.status_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
        aria_label=ui.tr("bead.bar.open-bluetooth"),on_activate=popouts and ui.action("status.pill.open","bluetooth") or nil,
        status_glyph("glyph",ui.bind("bead.desktop.bluetooth"),"bluetooth"),
        content and status_text{id="value",text=content,width=width,font_size=t.detail_font,color="secondary"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local w,h=visual.slot(props.instance,320,220)
    local content=ui.Column{id="connected-devices",width="fill",gap=kit.tokens.spacing.sm,
        ui.Repeat{id="devices",model=ui.bind("bluetooth.devices"),key="id",render=function(device)
            if not device.active then return nil end
            return ui.Row{id="device",width="fill",height=44,gap=kit.tokens.spacing.sm,align="center",
                ui.Box{id="device-icon-wrap",width=34,height=34,radius=kit.tokens.radii.small,fill=ui.alpha("primary",.18),
                    ui.Text{id="device-icon",text=device.icon or "headphones",family="Material Symbols Rounded",font_size=kit.tokens.icons.small,width=34,height=34,text_align="center",color="primary"}},
                ui.Column{id="device-copy",width=0,grow=1,height=40,justify="center",
                    ui.Text{id="device-name",text=device.label,width="fill",height=22,font_size=kit.tokens.text.body*4/3,color="onSurface"},
                    ui.Text{id="device-caption",text=device.caption,width="fill",height=16,font_size=kit.tokens.text.caption*4/3,color="onSurfaceVariant"}}}
        end}}
    return visual.glass("bluetooth-desktop",w,h,kit.tokens.spacing.lg,{
        ui.Column{id="device-card",width="fill",height="fill",gap=kit.tokens.spacing.md,
            ui.Row{id="heading",width="fill",height=28,gap=kit.tokens.spacing.sm,align="center",
                ui.Text{id="title",text="Bluetooth",width=0,grow=1,font_size=kit.tokens.text.title*4/3,color="onSurface"},
                kit.Switch{id="radio-toggle",checked=ui.bind("bluetooth.power_checked"),aria_label="Bluetooth",
                    on_activate=ui.action("bluetooth.toggle_power")}},
            content}})
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="bluetooth",title="Bluetooth",icon="bluetooth",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",3,bar)}},
        desktop={default_style="devices",styles={status={title="Stored status",min_span=1,max_span=1200,preferred_span=300,preferred_height=80,hidden=true,render=function(p,s,d) p.form_style="quiet"; return desktop(p,s,d) end},
            devices={title="Device stack",min_span=1,max_span=1200,preferred_span=320,preferred_height=220,render=desktop},
            quiet={title="Quiet glass",min_span=1,max_span=1200,preferred_span=360,preferred_height=170,hidden=true,render=desktop}}},
        panel={styles={bluetooth={view="bluetooth"}}}}
}
