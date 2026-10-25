-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local function bar(props,settings,_data)
    local label=props.form_style=="icon+text"
    return (popouts and kit.Button or ui.Row){id="power",height=t.status_height,width=label and 92 or t.status_ink.power+2*t.status_padding,justify="center",direction=popouts and "row" or nil,gap=label and t.item_gap or nil,align="center",padding=0,padding_left=t.status_padding,padding_right=t.status_padding,fill=popouts and "transparent" or nil,
        aria_label=popouts and ui.tr("bead.bar.open-session") or nil,on_activate=popouts and ui.action("view.open","session") or nil,
        status_glyph("glyph","power_settings_new","power","error"),label and ui.Text{id="label",text="Power",font_size=t.detail_font,color="error"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=280,h=80}
    return kit.Button{id="power-desktop",width=slot.w,height=slot.h,padding=kit.tokens.spacing.md,fill="surfaceContainer",radius=kit.tokens.radii.medium,
        on_activate=ui.action("view.open","session"),ui.Row{id="content",width="fill",height="fill",gap=kit.tokens.spacing.md,align="center",
            status_glyph("glyph","power_settings_new","power","error"),ui.Text{id="label",text="Power",font_size=20,color="error"}}}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="power",title="Power",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="button",styles={button={title="Button",min_span=1,max_span=1200,preferred_span=220,render=desktop}}},
        panel={styles={session={view="session"}}}}
}
