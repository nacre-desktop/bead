-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local status_text=c.status_text
local function bar(props,settings,_data)
    local style=props.form_style
    local content=style=="icon+value" and ui.bind("bead.desktop.audio.label") or style=="icon+text" and "Audio" or nil
    local value=ui.resolve_for_check(ui.bind("bead.desktop.audio.label")) or ""
    local width=content and ui.text_width(style=="icon+value" and value or content,"IBM Plex Sans",t.detail_font) or 0
    return kit.StatusIcon{id="audio",height=t.status_height,padding=0,padding_left=t.status_padding,padding_right=t.status_padding,align="center",justify="center",fill="transparent",hover_fill="surfaceContainerHigh",aria_label=popouts and ui.tr("bead.bar.open-sound") or ui.tr("bead.bar.audio-hint"),on_activate=popouts and ui.action("status.pill.open","audio") or ui.action("services.audio"),
        status_glyph("glyph",ui.bind("bead.desktop.audio"),"audio"),
        content and status_text{id="value",text=content,width=width,font_size=t.detail_font,color="secondary"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=280,h=80}
    return ui.Box{id="audio-desktop",width=slot.w,height=slot.h,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.medium,fill="surfaceContainer",on_activate=popouts and ui.action("status.pill.open","audio") or ui.action("services.audio"),
        ui.Row{id="content",width="fill",height="fill",gap=kit.tokens.spacing.md,align="center",
            status_glyph("glyph",ui.bind("bead.desktop.audio"),"audio"),
            ui.Column{id="labels",width="fill",gap=kit.tokens.spacing.xs,ui.Text{id="title",text="Sound",font_size=kit.tokens.text.body*4/3,color="onSurface"},
                ui.Text{id="value",text=ui.bind("bead.desktop.audio.label"),font_size=kit.tokens.text.title*4/3,color="primary"}},
        }}
end
local function style(title,preferred,render)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render}
end
return {id="sound",title="Sound",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="level",styles={level={title="Level",min_span=1,max_span=1200,preferred_span=260,render=desktop}}},
        panel={styles={audio={view="audio"}}}}
}
