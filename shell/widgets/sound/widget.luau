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
    local w,h=visual.slot(props.instance,260,340)
    local label=ui.resolve_for_check(ui.bind("bead.desktop.audio.label")) or "0%"
    local level=tonumber(string.match(tostring(label),"%d+")) or 0
    local device=ui.resolve_for_check(ui.bind("bead.desktop.audio.device")) or "Sound output"
    local fraction=math.clamp(level,0,100)/100
    local ring=ui.Box{id="audio-ring",width=170,height=170,fill="transparent",
        ui.Arc{id="track",position="absolute",left=7,top=7,width=156,height=156,start_angle=135,sweep_angle=270,
            stroke_width=12,color=ui.alpha("onSurface",.12),round_caps=true},
        ui.Arc{id="level",position="absolute",left=7,top=7,width=156,height=156,start_angle=135,
            sweep_angle=ui.animate("audio-sweep",fraction*270,.24,"out-cubic"),stroke_width=12,color="primary",round_caps=true},
        ui.Text{id="reading",position="absolute",left=0,top=62,width=170,height=48,text=tostring(level),family=visual.display_font,
            font_weight=visual.display_weight,font_size=kit.tokens.text.headline*4/3,text_align="center",color=visual.text}}
    local action=ui.action("services.audio")
    return visual.glass("audio-desktop",w,h,kit.tokens.spacing.lg,{
        ui.Column{id="content",width="fill",height="fill",align="center",justify="center",gap=kit.tokens.spacing.sm,
            ring,
            ui.Text{id="device",text=device,width="fill",height=22,font_size=kit.tokens.text.body*4/3,text_align="center",color="onSurfaceVariant"},
            ui.Row{id="actions",height=36,gap=kit.tokens.spacing.sm,justify="center",
                kit.Button{id="mute",text="Mute",height=36,padding_left=kit.tokens.spacing.md,padding_right=kit.tokens.spacing.md,radius=kit.tokens.radii.full,fill=ui.alpha("onSurface",.08),focus_on_pointer=false,on_activate=action},
                kit.Button{id="output",text="Output",height=36,padding_left=kit.tokens.spacing.md,padding_right=kit.tokens.spacing.md,radius=kit.tokens.radii.full,fill=ui.alpha("onSurface",.08),focus_on_pointer=false,on_activate=action}}}})
end
local function style(title,preferred,render)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render}
end
return {id="sound",title="Sound",icon="volume_up",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="ring",styles={level={title="Stored level",min_span=1,max_span=1200,preferred_span=260,preferred_height=96,hidden=true,render=function(p,s,d) p.form_style="ring"; return desktop(p,s,d) end},
            ring={title="Volume ring",min_span=1,max_span=1200,preferred_span=320,preferred_height=176,render=function(p,s,d) p.form_style="ring"; return desktop(p,s,d) end},
            pearl={title="Pearl dial",min_span=1,max_span=1200,preferred_span=320,preferred_height=176,hidden=true,render=function(p,s,d) p.form_style="pearl"; return desktop(p,s,d) end},
            frameless={title="Wallpaper type",min_span=1,max_span=1200,preferred_span=420,preferred_height=176,hidden=true,render=function(p,s,d) p.form_style="frameless"; return desktop(p,s,d) end}}},
        panel={styles={audio={view="audio"}}}}
}
