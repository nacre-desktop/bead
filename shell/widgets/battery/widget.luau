-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local centered_text=c.centered_text
local popouts=c.popouts
local glyph=c.glyph
return {id="battery",title="Battery",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return (popouts and kit.StatusIcon or ui.Row){id="battery",direction="row",height=t.status_height,gap=t.item_gap,padding=0,padding_left=t.item_padding,padding_right=t.item_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-battery"),on_activate=popouts and ui.action("status.pill.open","battery") or nil,
                glyph("glyph",ui.bind("bead.desktop.battery")),
                centered_text{id="battery-value",text=ui.bind("bead.desktop.battery.value"),width=ui.bind("bar.battery.detail.width"),font_size=t.detail_font,color="secondary"},
            }
    end}}}
