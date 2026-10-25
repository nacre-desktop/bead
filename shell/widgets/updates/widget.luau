-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local centered_text=c.centered_text
local popouts=c.popouts
local updates_on=c.updates_on
local updates_n=c.updates_n
local glyph=c.glyph
return {id="updates",title="Updates",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return updates_on and updates_n>0 and kit.Button{id="updates",direction="row",padding=0,fill="transparent",hover_fill="surfaceContainerHigh",on_activate=popouts and ui.action("status.pill.open","updates") or nil,height=30,gap=4,shrink=0,padding_left=3,padding_right=3,align="center",
                glyph("package","package_2","primary"),
                centered_text{id="count",text=ui.bind("bar.updates.text"),width=t.update_digit_width*#tostring(updates_n)+t.update_text_inset,height=24,font_size=16,color="primary"},
            } or ui.Box{id="updates",width=0,height=0,shrink=0,visible=false}
    end}}}
