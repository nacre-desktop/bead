-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local status_text=c.status_text
local popouts=c.popouts
local updates_on=c.updates_on
local updates_n=c.updates_n
local status_glyph=c.status_glyph
local function bar(props,settings,_data)
    local style=props.form_style
    local content=style=="icon+value" and ui.bind("bar.updates.text") or style=="icon+text" and "Updates" or nil
    local width=content and ui.text_width(style=="icon+value" and (ui.resolve_for_check(content) or "0") or content,"IBM Plex Sans",t.clock_font) or 0
    local action=popouts and ui.action("status.pill.open","updates") or nil
    return updates_on and updates_n>0 and kit.Button{id="updates",direction="row",padding=0,fill="transparent",hover_fill="surfaceContainerHigh",on_activate=action,height=t.status_height,gap=t.item_gap,shrink=0,padding_left=t.status_padding,padding_right=t.status_count_padding,align="center",
        status_glyph("package","package_2","package","primary"),
        content and status_text{id="count",text=content,width=width,font_size=t.clock_font,color="primary"} or nil,
    } or ui.Box{id="updates",width=0,height=0,shrink=0,visible=false}
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=280,h=80}
    local width=slot.w
    local height=slot.h
    return ui.Box{id="updates-desktop",width=width,height=height,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.medium,fill="surfaceContainer",on_activate=popouts and ui.action("status.pill.open","updates") or nil,
        ui.Row{id="content",width="fill",height="fill",gap=kit.tokens.spacing.md,align="center",
            status_glyph("package","package_2","package","primary"),
            ui.Column{id="labels",width="fill",gap=kit.tokens.spacing.xs,ui.Text{id="title",text="Updates",font_size=kit.tokens.text.body*4/3,color="onSurface"},
                ui.Text{id="count",text=ui.bind("bar.updates.text"),font_size=kit.tokens.text.title*4/3,color="primary"}},
        }}
end
local function style(title,preferred,render)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render}
end
return {id="updates",title="Updates",icon="package_2",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon+value",styles={
        icon=style("Icon",1,bar),["icon+value"]=style("Icon + value",2,bar),["icon+text"]=style("Icon + text",3,bar),
    }},desktop={default_style="summary",styles={summary={title="Summary",min_span=1,max_span=1200,preferred_span=280,render=desktop}}},
        panel={styles={updates={view="updates"}}}}
}
