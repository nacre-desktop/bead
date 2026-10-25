-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local glyph=c.glyph
return {id="power",title="Power",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return (popouts and kit.Button or ui.Row){id="power",height=30,width=30,justify="center",direction=popouts and "row" or nil,align="center",padding=popouts and 0 or nil,fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-session") or nil,on_activate=popouts and ui.action("view.open","session") or nil,
                glyph("glyph","power_settings_new","error"),
            }
    end}}}
