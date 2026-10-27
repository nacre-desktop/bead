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
    local style=props.form_style
    local content=style=="icon+value" and ui.bind("bead.desktop.audio.label") or style=="icon+text" and "Audio" or nil
    local value=ui.resolve_for_check(ui.bind("bead.desktop.audio.label")) or ""
    local width=content and ui.text_width(style=="icon+value" and value or content,"IBM Plex Sans",t.detail_font) or 0
    return kit.StatusIcon{id="audio",direction="row",height=t.status_height,gap=style~="icon" and t.item_gap or 0,padding=0,padding_left=t.status_padding,padding_right=t.status_padding,align="center",justify="center",fill="transparent",hover_fill="surfaceContainerHigh",aria_label=popouts and ui.tr("bead.bar.open-sound") or ui.tr("bead.bar.audio-hint"),on_activate=popouts and ui.action("status.pill.open","audio") or ui.action("services.audio"),
        status_glyph("glyph",ui.bind("bead.desktop.audio"),"audio"),
        content and status_text{id="value",text=content,width=width,font_size=t.detail_font,color="secondary"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local w,h=visual.slot(props.instance,320,176)
    local label=ui.resolve_for_check(ui.bind("bead.desktop.audio.label")) or "0%"
    local level=tonumber(string.match(tostring(label),"%d+")) or 0
    local style=props.form_style or "ring"
    local action=popouts and ui.action("status.pill.open","audio") or ui.action("services.audio")
    local device=ui.resolve_for_check(ui.bind("bead.desktop.audio.device")) or "Sound output"
    if style=="frameless" then
        return ui.Row{id="audio-wallpaper",width=w,height=h,fill="transparent",align="center",gap=kit.tokens.spacing.lg,
            visual.ring("audio-ring",124,level,"Volume"),ui.Column{id="audio-copy",width="fill",height="fill",justify="center",gap=kit.tokens.spacing.sm,
                ui.Text{id="device",text=device,width="fill",height=26,font_size=kit.tokens.text.title*4/3,color="primary"},
                ui.Text{id="value",text=label,width="fill",height=56,font_size=kit.tokens.text.headline*4/3,color="onSurface"}}}
    end
    return ui.Box{id="audio-desktop",width=w,height=h,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.large,
        fill=style=="pearl" and "surfaceContainerHigh" or "surfaceContainer",on_activate=action,
        ui.Row{id="content",width="fill",height="fill",gap=kit.tokens.spacing.md,align="center",
            visual.ring("audio-ring",124,level,"Volume"),ui.Column{id="labels",width="fill",justify="center",gap=kit.tokens.spacing.sm,
                ui.Text{id="title",text=device,font_size=kit.tokens.text.title*4/3,color="onSurface"},
                ui.Text{id="value",text=label,font_size=kit.tokens.text.headline*4/3,color="primary"},
                kit.Slider{id="volume",label="Volume",value=ui.bind("bead.desktop.audio.percent"),min=0,max=100,step=1,width=150,on_change="services.audio.volume"},
                kit.Button{id="mute",text="Mute",height=32,focus_on_pointer=false,on_activate=ui.action("services.audio")}}}}
end
local function style(title,preferred,render)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render}
end
return {id="sound",title="Sound",icon="volume_up",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="ring",styles={level={title="Stored level",min_span=1,max_span=1200,preferred_span=260,preferred_height=96,hidden=true,render=function(p,s,d) p.form_style="ring"; return desktop(p,s,d) end},
            ring={title="Volume ring",min_span=1,max_span=1200,preferred_span=320,preferred_height=176,render=function(p,s,d) p.form_style="ring"; return desktop(p,s,d) end},
            pearl={title="Pearl dial",min_span=1,max_span=1200,preferred_span=320,preferred_height=176,render=function(p,s,d) p.form_style="pearl"; return desktop(p,s,d) end},
            frameless={title="Wallpaper type",min_span=1,max_span=1200,preferred_span=420,preferred_height=176,render=function(p,s,d) p.form_style="frameless"; return desktop(p,s,d) end}}},
        panel={styles={audio={view="audio"}}}}
}
