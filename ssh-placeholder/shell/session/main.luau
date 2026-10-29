-- SPDX-License-Identifier: GPL-3.0-only
-- Power dropdown: ordered actions and inline confirmation on shared kit components.
-- The session route and every power widget share this lazy, anchored view.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local enabled=get("session.enabled") and not get("session.busy")
local question=get("session.confirm")
local asking=get("session.confirm_action")
local W=get("view.width")
local PAD,GAP,HEAD=T.spacing.panel,T.spacing.compact,T.spacing.huge
local INNER=W-2*PAD
local BODY,CAPTION=T.text.body*4/3,T.text.caption*4/3
local TILE_H=T.spacing.huge+T.spacing.sm
local entries={
    {"lock",ui.tr("bead.panel.session-lock"),"lock"},{"logout",ui.tr("bead.panel.session-logout"),"logout"},
    {"suspend",ui.tr("bead.panel.session-suspend"),"bedtime"},
    {"reboot",ui.tr("bead.panel.session-reboot"),"restart_alt"},{"shutdown",ui.tr("bead.panel.session-shutdown"),"power_settings_new"},
}
-- Everything is placed absolutely in the panel, so the focusable tiles keep the stable ids `act-<key>.main`
-- (the host hands focus back to the asking tile on Esc).
local TOP=PAD
local children={
    ui.Row{id="header",position="absolute",left=PAD,top=TOP,width=INNER,height=HEAD,gap=T.spacing.sm,align="center",
        ui.Text{id="title",text=ui.tr("bead.panel.control-power-title"),height=HEAD,grow=1,shrink=1,font_size=T.text.title*4/3,color="onSurface"}},
}
local rows=#entries
local y=TOP+HEAD+T.spacing.lg
for i,entry in ipairs(entries) do
    local key,label,glyph=entry[1],entry[2],entry[3]
    children[#children+1]=kit.ToggleTile{id="act-"..key,position="absolute",left=PAD,top=y+(i-1)*(TILE_H+GAP),width=INNER,height=TILE_H,
        icon=glyph,label=label,on=asking==key,available=enabled or asking==key,aria_label=label,
        on_activate=enabled and (asking==key and ui.action("session.confirm") or ui.action("session.select",key)) or nil}
end
y=y+rows*TILE_H+(rows-1)*GAP+T.spacing.lg
if question~="" and asking~="" then
    -- Inline confirmation: the question, then Cancel and the action. Enter on the tile or the button confirms,
    -- Esc or the timeout reverts.
    local label=question
    for _,entry in ipairs(entries) do if entry[1]==asking then label=entry[2] end end
    local half=(INNER-2*T.spacing.md-T.spacing.sm)/2
    children[#children+1]=kit.Card{id="confirmation",position="absolute",left=PAD,top=y,width=INNER,height=104,shrink=0,padding=T.spacing.md,gap=T.spacing.md,radius=T.radii.medium,
        ui.Text{id="question",text=ui.bind("session.confirm"),width=INNER-2*T.spacing.md,height=28,font_size=T.text.title*4/3,color="onSurface"},
        ui.Row{id="buttons",width=INNER-2*T.spacing.md,height=40,gap=T.spacing.sm,
            kit.Button{id="cancel",width=half,height=40,padding=0,radius=T.radii.full,aria_label=ui.tr("bead.common.cancel"),on_activate=ui.action("session.cancel"),
                ui.Row{id="cancel-row",width=half,height=40,align="center",justify="center",
                    ui.Text{id="cancel-label",text=ui.tr("bead.common.cancel"),width=half-T.spacing.md,height=22,text_align="center",font_size=BODY,color="onSurface"}}},
            kit.Button{id="confirm",width=half,height=40,padding=0,radius=T.radii.full,fill="primary",aria_label=question,on_activate=enabled and ui.action("session.confirm") or nil,
                ui.Row{id="confirm-row",width=half,height=40,align="center",justify="center",
                    ui.Text{id="confirm-label",text=label,width=half-T.spacing.md,height=22,text_align="center",font_size=BODY,color="onPrimary"}}}}}
    y=y+104+T.spacing.lg
end
children[#children+1]=ui.Text{id="message",position="absolute",left=PAD,top=y,width=INNER,height=CAPTION*3,font_size=CAPTION,color="error",text=ui.bind("session.message")}
return kit.Panel{id="session-popout",edge=get("view.edge"),open=ui.bind("panel.open"),duration=get("view.duration"),transition="ease-in-out",fade=false,reveal=true,
    width=W,height=y+(get("session.message")~="" and CAPTION*3 or 0)+PAD,padding=0,radius=T.radii.medium,fill="surface",table.unpack(children)}
