-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local status_text=c.status_text
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
return {id="clock",title="Clock",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return (popouts and kit.Button or ui.Row){id="clock",height=t.status_height,direction=popouts and "row" or nil,gap=t.item_gap,padding=0,padding_left=t.status_padding,padding_right=t.status_count_padding,align="center",fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-calendar") or nil,on_activate=popouts and ui.action("view.open","calendar") or nil,
                status_glyph("calendar","calendar_month","calendar","tertiary"),
                status_text{id="time",text=ui.bind("bar.clock.text"),width=math.max(t.clock_min_width,ui.text_width(ui.resolve_for_check(ui.bind("bar.clock.text")),"IBM Plex Sans",t.clock_font)),shrink=0,font_size=t.clock_font,color="tertiary"},
            }
    end}}}
