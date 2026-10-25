-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
return {id="power",title="Power",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return (popouts and kit.Button or ui.Row){id="power",height=t.status_height,width=t.status_ink.power+2*t.status_padding,justify="center",direction=popouts and "row" or nil,align="center",padding=0,fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-session") or nil,on_activate=popouts and ui.action("view.open","session") or nil,
                status_glyph("glyph","power_settings_new","power","error"),
            }
    end}}}
