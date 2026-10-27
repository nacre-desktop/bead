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
    local rows=ui.resolve_for_check(ui.bind("network.rows")) or {}
    local network="No network connected"
    local strength=0
    for _,item in rows do if item.group=="connected" then network=item.label or "Connected"; strength=item.strength or 0; break end end
    local style=props.form_style or "glass"
    local action=popouts and ui.action("status.pill.open","network") or nil
    local signal=ui.animate("wifi-signal",math.clamp(tonumber(strength) or 0,0,100),.2,"out-cubic")
    local signal_width=math.max(0,(slot.w-2*kit.tokens.spacing.md)*signal/100)
    if style=="wallpaper" then
        return ui.Row{id="network-wallpaper",width=slot.w,height=slot.h,fill="transparent",align="center",gap=kit.tokens.spacing.lg,
            ui.Text{id="signal-icon",text=glyph,family="Material Symbols Rounded",width=72,height=72,font_size=kit.tokens.icons.hero,color="primary"},
            ui.Column{id="network-copy",width=0,grow=1,height="fill",justify="center",gap=kit.tokens.spacing.xs,
                ui.Text{id="title",text="Wi-Fi",width="fill",height=26,font_size=kit.tokens.text.title*4/3,color="primary"},
                ui.Text{id="network",text=network,width="fill",height=42,font_size=kit.tokens.text.headline*4/3,color="onSurface"},
                ui.Text{id="strength",text=tostring(signal).."% signal",width="fill",height=22,font_size=kit.tokens.text.body*4/3,color="onSurfaceVariant"}}}
    end
    return ui.Box{id="network-desktop",width=slot.w,height=slot.h,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.large,
        fill=style=="pearl" and "surfaceContainerHigh" or "surfaceContainer",on_activate=action,
        ui.Column{id="network-card",width="fill",height="fill",gap=kit.tokens.spacing.sm,
            ui.Row{id="heading",width="fill",height=28,gap=kit.tokens.spacing.sm,align="center",
                status_glyph("glyph",ui.bind("bead.desktop.network"),"network"),
                ui.Text{id="title",text="Wi-Fi",width=0,grow=1,font_size=kit.tokens.text.title*4/3,color="onSurface"},
                kit.Switch{id="radio-toggle",checked=ui.bind("network.wifi_enabled"),aria_label="Wi-Fi",on_activate=ui.action("network.toggle_wifi")}},
            ui.Text{id="network",text=network,width="fill",height=32,font_size=kit.tokens.text.headline*4/3,color="primary"},
            ui.Row{id="signal-row",width="fill",height=12,radius=6,fill=ui.alpha("onSurface",.14),
                ui.Box{id="signal",width=signal_width,height=12,radius=6,fill="primary"}}}}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="wifi",title="Wi-Fi",icon="wifi",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="glass",styles={status={title="Stored status",min_span=1,max_span=1200,preferred_span=300,preferred_height=80,hidden=true,render=function(p,s,d) p.form_style="glass"; return desktop(p,s,d) end},
            glass={title="Network card",min_span=1,max_span=1200,preferred_span=320,preferred_height=154,render=function(p,s,d) p.form_style="glass"; return desktop(p,s,d) end},
            pearl={title="Pearl panel",min_span=1,max_span=1200,preferred_span=320,preferred_height=154,render=function(p,s,d) p.form_style="pearl"; return desktop(p,s,d) end},
            wallpaper={title="Wallpaper signal",min_span=1,max_span=1200,preferred_span=420,preferred_height=130,render=function(p,s,d) p.form_style="wallpaper"; return desktop(p,s,d) end}}},
        panel={styles={network={view="network"}}}}
}
