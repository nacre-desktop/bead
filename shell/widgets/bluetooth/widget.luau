-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
return {id="bluetooth",title="Bluetooth",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return (popouts and kit.StatusIcon or ui.Row){id="bluetooth",height=t.status_height,padding=0,padding_left=t.status_padding,padding_right=t.status_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-bluetooth"),on_activate=popouts and ui.action("status.pill.open","bluetooth") or nil,
                status_glyph("glyph",ui.bind("bead.desktop.bluetooth"),"bluetooth"),
            }
    end}}}
