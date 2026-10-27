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
    local w,h=visual.slot(props.instance,320,292)
    local actions={{"lock","Lock","lock"},{"suspend","Sleep","bedtime"},{"reboot","Restart","restart_alt"},{"shutdown","Shut down","power_settings_new"}}
    local buttons={}
    local host=ui.resolve_for_check(ui.bind("dashboard.host")) or {}
    local account,uptime
    for _,row in host do
        if row.id=="badge" then account=row.text end
        if row.id=="timer" then uptime=row.text end
    end
    local enabled=ui.resolve_for_check(ui.bind("session.enabled"))==true and ui.resolve_for_check(ui.bind("session.busy"))~=true
    local asking=ui.resolve_for_check(ui.bind("session.confirm_action")) or ""
    for _,item in actions do
        table.insert(buttons,kit.Button{id=item[1],width=0,grow=1,height=64,padding=kit.tokens.spacing.xs,
            radius=kit.tokens.radii.medium,fill=item[1]=="shutdown" and ui.alpha("error",.2) or ui.alpha("onSurface",.07),focus_on_pointer=false,
            aria_label=item[2],on_activate=enabled and (asking==item[1] and ui.action("session.confirm") or ui.action("session.select",item[1])) or nil,
            ui.Column{id="action-content",width="fill",height="fill",gap=2,align="center",justify="center",
                ui.Text{id="icon",text=item[3],family="Material Symbols Rounded",width=24,height=24,font_size=20,text_align="center",color=item[1]=="shutdown" and "error" or visual.text},
                ui.Text{id="label",text=item[2],width="fill",height=20,font_size=kit.tokens.text.body*4/3,text_align="center",color=item[1]=="shutdown" and "error" or visual.text}}})
    end
    return visual.reference_glass("power-desktop",w,h,kit.tokens.spacing.lg,{
        ui.Column{id="power-layout",width="fill",height="fill",gap=kit.tokens.spacing.md,
            ui.Row{id="heading",width="fill",height=48,align="center",gap=kit.tokens.spacing.sm,
                ui.Text{id="account-icon",text="account_circle",family="Material Symbols Rounded",width=36,height=36,font_size=32,color="primary"},
                ui.Column{id="account-details",width=200,height=40,gap=0,justify="center",
                    ui.Text{id="title",text=account or "User",font_size=kit.tokens.text.body*4/3,color=visual.text},
                    ui.Text{id="uptime",text=uptime or "Uptime unavailable",font_size=kit.tokens.text.caption*4/3,color=visual.secondary}},
            },
            ui.Column{id="actions",width="fill",height=0,grow=1,gap=kit.tokens.spacing.sm,
                ui.Row{id="actions-top",width="fill",height=64,gap=kit.tokens.spacing.sm,table.unpack(buttons,1,2)},
                ui.Row{id="actions-bottom",width="fill",height=64,gap=kit.tokens.spacing.sm,table.unpack(buttons,3,4)}}}})
end
local function style(title,preferred,render) return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=render} end
return {id="power",title="Power",icon="power_settings_new",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="icon",styles={icon=style("Icon",1,bar),["icon+text"]=style("Icon + text",2,bar)}},
        desktop={default_style="actions",styles={button={title="Stored button",min_span=1,max_span=1200,preferred_span=220,preferred_height=92,hidden=true,render=function(p,s,d) p.form_style="actions"; return desktop(p,s,d) end},
            actions={title="Actions card",min_span=1,max_span=1200,preferred_span=320,preferred_height=292,render=desktop},
            compact={title="Quiet row",min_span=1,max_span=1200,preferred_span=320,preferred_height=92,hidden=true,render=function(p,s,d) p.form_style="compact"; return desktop(p,s,d) end}}},
        panel={styles={session={view="session"}}}}
}
