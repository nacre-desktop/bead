-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local glyph=c.glyph
return {id="wifi",title="Wifi",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return popouts and kit.StatusIcon{id="network",width=30,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-network"),on_activate=ui.action("status.pill.open","network"),
                glyph("glyph",ui.bind("bead.desktop.network")),
            } or glyph("network",ui.bind("bead.desktop.network"))
    end}}}
