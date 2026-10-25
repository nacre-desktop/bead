-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local centered_text=c.centered_text
local popouts=c.popouts
local glyph=c.glyph
return {id="clock",title="Clock",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return (popouts and kit.Button or ui.Row){id="clock",height=30,direction=popouts and "row" or nil,gap=4,padding_left=3,padding_right=3,align="center",fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-calendar") or nil,on_activate=popouts and ui.action("view.open","calendar") or nil,
                glyph("calendar","calendar_month","tertiary"),
                centered_text{id="time",text=ui.bind("bar.clock.text"),width=ui.bind("bar.clock.width"),shrink=0,font_size=16,height=24,family="IBM Plex Sans",color="tertiary"},
            }
    end}}}
