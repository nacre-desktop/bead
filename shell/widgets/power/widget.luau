-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local visual=require("../desktop-visuals")
local function bar(props,settings,_data)
    local label=props.form_style=="icon+text"
    return (popouts and kit.Button or ui.Row){id="power",height=t.status_height,width=label and 92 or t.status_ink.power+2*t.status_padding,justify="center",direction=popouts and "row" or nil,gap=label and t.item_gap or nil,align="center",padding=0,padding_left=t.status_padding,padding_right=t.status_padding,fill=popouts and "transparent" or nil,
        aria_label=popouts and ui.tr("bead.bar.open-session") or nil,on_activate=popouts and ui.action("view.open","session") or nil,
        status_glyph("glyph","power_settings_new","power","error"),label and ui.Text{id="label",text="Power",font_size=t.detail_font,color="error"} or nil,
    }
end
local function desktop(props,_settings,_data)
    local w,h=visual.slot(props.instance,360,112)
    local style=props.form_style or "actions"
    local actions={{"lock","Lock","lock"},{"suspend","Sleep","bedtime"},{"reboot","Restart","restart_alt"},{"shutdown","Shut down","power_settings_new"}}
    local buttons={}
    local enabled=ui.resolve_for_check(ui.bind("session.enabled"))==true and ui.resolve_for_check(ui.bind("session.busy"))~=true
    local asking=ui.resolve_for_check(ui.bind("session.confirm_action")) or ""
    for _,item in actions do
        table.insert(buttons,kit.Button{id=item[1],width=style=="compact" and 68 or 76,height=58,padding=kit.tokens.spacing.xs,
            radius=kit.tokens.radii.medium,fill="surfaceContainerHigh",focus_on_pointer=false,
            aria_label=item[2],on_activate=enabled and (asking==item[1] and ui.action("session.confirm") or ui.action("session.select",item[1])) or nil,
            ui.Column{id="action",width="fill",height="fill",align="center",justify="center",gap=kit.tokens.spacing.xs,
                ui.Text{id="icon",text=item[1],family="Material Symbols Rounded",font_size=kit.tokens.icons.medium,color="primary"},
                ui.Text{id="label",text=item[2],width="fill",height=18,font_size=kit.tokens.text.caption*4/3,text_align="center",color="onSurface"}}})
    end
    return ui.Box{id="power-desktop",width=w,height=h,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.large,
        fill="surfaceContainer",ui.Column{id="power-layout",width="fill",height="fill",gap=kit.tokens.spacing.sm,
            ui.Row{id="heading",width="fill",height=28,gap=kit.tokens.spacing.sm,align="center",
                status_glyph("glyph","power_settings_new","power","error"),ui.Text{id="title",text="Power",font_size=kit.tokens.text.title*4/3,color="onSurface"}},
            ui.Row{id="actions",width="fill",height="fill",gap=kit.tokens.spacing.xs,align="center",table.unpack(buttons)}}}
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="power",title="Power",icon="power_settings_new",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="actions",styles={actions={title="Action card",min_span=1,max_span=1200,preferred_span=360,preferred_height=112,render=function(p,s,d) p.form_style="actions"; return desktop(p,s,d) end},
            compact={title="Quiet row",min_span=1,max_span=1200,preferred_span=320,preferred_height=92,render=function(p,s,d) p.form_style="compact"; return desktop(p,s,d) end}}},
        panel={styles={session={view="session"}}}}
}
