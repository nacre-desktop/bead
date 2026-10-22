-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local enabled=ui.resolve_for_check(ui.bind("session.enabled")) and not ui.resolve_for_check(ui.bind("session.busy"))
local question=ui.resolve_for_check(ui.bind("session.confirm"))
local padding=kit.tokens.spacing.md
local entries={
    {"lock","Lock","lock"},{"logout","Log out","logout"},{"suspend","Suspend","bedtime"},
    {"reboot","Restart","restart_alt"},{"shutdown","Power off","power_settings_new"},
}
local children={}
if question~="" then
    table.insert(children,ui.Text{id="question",text=ui.bind("session.confirm"),height=48,font_size=kit.tokens.text.body*4/3,color="onSurface"})
    table.insert(children,kit.Button{id="confirm",aria_label="Confirm",on_activate=enabled and ui.action("session.confirm") or nil,
        ui.Text{id="confirm-label",text="Confirm",height=24,font_size=kit.tokens.text.body*4/3,color="error"}})
    table.insert(children,kit.Button{id="cancel",aria_label="Cancel",on_activate=ui.action("session.cancel"),
        ui.Text{id="cancel-label",text="Cancel",height=24,font_size=kit.tokens.text.body*4/3,color="onSurface"}})
else
    for _,entry in ipairs(entries) do
        local key,label,glyph=entry[1],entry[2],entry[3]
        table.insert(children,kit.Button{id="act-"..key,aria_label=label,on_activate=enabled and ui.action("session.select",key) or nil,
            ui.Row{id="row-"..key,height=24,gap=kit.tokens.spacing.md,align="center",
                ui.Text{id="glyph-"..key,text=glyph,width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,
                    color=key=="shutdown" and "error" or (enabled and "onSurface" or "onSurfaceVariant")},
                ui.Text{id="label-"..key,text=label,height=24,grow=1,font_size=kit.tokens.text.body*4/3,
                    color=enabled and "onSurface" or "onSurfaceVariant"}}})
    end
end
table.insert(children,ui.Text{id="message",text=ui.bind("session.message"),font_size=kit.tokens.text.caption*4/3,height=40,color="error"})
return kit.Panel{id="session-popout",edge=ui.resolve_for_check(ui.bind("view.edge")),
    open=ui.bind("panel.open"),duration=ui.resolve_for_check(ui.bind("view.duration")),
    width=ui.resolve_for_check(ui.bind("view.width")),height=ui.resolve_for_check(ui.bind("view.height")),
    padding=padding,gap=kit.tokens.spacing.sm,radius=kit.tokens.radii.medium,fill="surface",
    table.unpack(children)}
