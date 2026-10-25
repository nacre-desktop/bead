-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
return {id="sound",title="Sound",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return kit.StatusIcon{id="audio",height=t.status_height,padding=0,padding_left=t.status_padding,padding_right=t.status_padding,align="center",justify="center",fill="transparent",hover_fill="surfaceContainerHigh",aria_label=popouts and ui.tr("bead.bar.open-sound") or ui.tr("bead.bar.audio-hint"),on_activate=popouts and ui.action("status.pill.open","audio") or ui.action("services.audio"),
                status_glyph("glyph",ui.bind("bead.desktop.audio"),"audio"),
            }
    end}}}
