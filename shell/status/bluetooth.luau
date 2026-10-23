-- SPDX-License-Identifier: GPL-3.0-only
-- Bluetooth popout: status summary and power switch (B0018), device list and pairing page (B0055)
-- on kit components. `bluetooth.view` (list|pair) is the flow state the adapter owns and
-- `bluetooth.pair_kind` selects the pairing prompt; every size comes from the size-scale tokens.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local ready=get("status.ready")
local active=get("status.active")
local visible=get("bluetooth.power_visible")
local enabled=get("bluetooth.power_enabled")
local view=get("bluetooth.view")
local padding=T.spacing.md
local gap=T.spacing.sm
local icon_size=T.icons.xlarge*4/3
local width=get("view.width")
local duration=get("view.duration")
local BODY,CAPTION,TITLE=T.text.body*4/3,T.text.caption*4/3,T.text.title*4/3
local FIELD,ROW=40,56
if view=="pair" then
    -- Page children with their heights, so the panel height follows the content and tokens.
    local kids,heights={},{}
    local function add(node,h)
        table.insert(kids,node);table.insert(heights,h)
    end
    local kind=get("bluetooth.pair_kind")
    local busy=get("bluetooth.pair_busy")
    add(ui.Text{id="title",text=ui.bind("bluetooth.pair_title"),height=24,font_size=TITLE,color="onSurface"},24)
    -- Wrapped text needs its lines up front: about 46 characters fit a line of the popout.
    local lines=math.min(3,math.max(1,math.ceil((utf8.len(get("bluetooth.pair_text")) or 0)/46)))
    add(ui.Text{id="prompt",text=ui.bind("bluetooth.pair_text"),height=lines*20,font_size=CAPTION,
        color=kind=="error" and "error" or "onSurfaceVariant"},lines*20)
    local respond=(not busy) and ui.action("bluetooth.pair_respond","accept") or nil
    if kind=="pin" or kind=="passkey" then
        local label=ui.tr(kind=="pin" and "bead.panel.bluetooth-pair-code" or "bead.panel.bluetooth-pair-passkey")
        add(kit.PasswordField{id="secret",height=FIELD,text=ui.bind("bluetooth.secret"),revealed=ui.bind("bluetooth.reveal"),
            placeholder=label,aria_label=label,
            show_label=ui.tr("bead.panel.network-password-show"),hide_label=ui.tr("bead.panel.network-password-hide"),
            on_toggle=ui.action("bluetooth.toggle_reveal"),on_activate=respond},FIELD)
    end
    if get("bluetooth.pair_error")~="" then
        add(ui.Text{id="pair-error",text=ui.bind("bluetooth.pair_error"),height=20,font_size=CAPTION,color="error"},20)
    end
    if get("bluetooth.pair_countdown")~="" then
        add(ui.Text{id="countdown",text=ui.bind("bluetooth.pair_countdown"),height=20,font_size=CAPTION,color="onSurfaceVariant"},20)
    end
    local cancel=ui.action("bluetooth.pair_cancel")
    local buttons
    if kind=="pin" or kind=="passkey" then
        buttons={kit.ActionButton{id="submit",label=ui.tr("bead.panel.bluetooth-pair"),primary=true,on_activate=respond},
            kit.ActionButton{id="cancel",label=ui.tr("bead.panel.bluetooth-pair-cancel"),on_activate=cancel}}
    elseif kind=="confirm" or kind=="authorize" then
        buttons={kit.ActionButton{id="accept",label=ui.tr(kind=="confirm" and "bead.panel.bluetooth-pair-confirm" or "bead.panel.bluetooth-pair-allow"),
                primary=true,on_activate=respond},
            kit.ActionButton{id="reject",label=ui.tr("bead.panel.bluetooth-pair-reject"),
                on_activate=(not busy) and ui.action("bluetooth.pair_respond","reject") or nil}}
    elseif kind=="error" and get("bluetooth.pair_retry") then
        buttons={kit.ActionButton{id="retry",label=ui.tr("bead.panel.bluetooth-pair-retry"),primary=true,on_activate=ui.action("bluetooth.pair_retry")},
            kit.ActionButton{id="cancel",label=ui.tr("bead.panel.bluetooth-pair-cancel"),on_activate=cancel}}
    else
        buttons={kit.ActionButton{id="cancel",label=ui.tr("bead.panel.bluetooth-pair-cancel"),on_activate=cancel}}
    end
    buttons.id="actions";buttons.height=36;buttons.gap=gap
    add(ui.Row(buttons),36)
    local total=2*padding+gap*(#heights-1)
    for _,h in heights do total=total+h end
    return kit.Panel{id="status-popout",edge=get("view.edge"),
        open=ui.bind("panel.open"),duration=duration,width=width,height=total,padding=padding,
        gap=gap,radius=T.radii.medium,fill="surface",table.unpack(kids)}
end
local text_width=width-padding*2-icon_size-T.spacing.md
-- The list grows the popout by its rows (and the message line while one shows).
local count=#get("bluetooth.devices")
local message=get("bluetooth.list_message")~=""
local height=get("view.height")+count*(ROW+gap)+(message and 20+gap or 0)
return kit.Panel{id="status-popout",edge=get("view.edge"),
    open=ui.bind("panel.open"),duration=duration,width=width,height=height,padding=padding,
    gap=gap,radius=T.radii.medium,fill="surface",
    ui.Text{id="heading",text=ui.bind("status.heading"),height=24,font_size=TITLE,color="onSurface"},
    ui.Row{id="summary",height=88,gap=T.spacing.md,align="center",
        ui.Text{id="icon",text=ui.bind("status.icon"),family="Material Symbols Rounded",font_size=icon_size,
            width=icon_size,height=42,shrink=0,color=ready and active and "primary" or "onSurfaceVariant"},
        ui.Column{width=text_width,height=84,gap=2,
            ui.Text{id="primary",text=ui.bind("status.primary"),font_size=BODY,height=60,color="onSurface"},
            ui.Text{id="secondary",text=ui.bind("status.secondary"),font_size=CAPTION,height=22,color="onSurfaceVariant"},
        },
    },
    ui.Text{id="detail",text=ui.bind("status.detail"),font_size=CAPTION,height=visible and 24 or 60,color="onSurfaceVariant"},
    visible and ui.Row{id="bluetooth-row",height=36,align="center",gap=T.spacing.md,
        ui.Text{id="bluetooth-label",text=ui.tr("bead.panel.bluetooth"),height=28,grow=1,font_size=BODY,
            color=enabled and "onSurface" or "onSurfaceVariant"},
        kit.Switch{id="bluetooth-power",checked=ui.bind("bluetooth.power_checked"),aria_label=ui.tr("bead.panel.bluetooth-radio"),
            on_activate=enabled and ui.action("bluetooth.toggle_power") or nil},
    } or nil,
    visible and ui.Text{id="bluetooth-message",text=ui.bind("bluetooth.power_message"),font_size=CAPTION,height=20,color="onSurfaceVariant"} or nil,
    ui.Repeat{id="devices",model=ui.bind("bluetooth.devices"),key="id",render=function(item)
        local ink=item.active and "onPrimaryContainer" or "onSurface"
        local quiet=item.active and "onPrimaryContainer" or "onSurfaceVariant"
        return ui.Row{id="device",height=ROW,padding_left=T.spacing.md,padding_right=T.spacing.sm,gap=T.spacing.md,align="center",
            radius=T.radii.small,fill=item.active and "primaryContainer" or "surfaceContainer",
            ui.Text{id="device-icon",text=item.icon,family="Material Symbols Rounded",font_size=T.icons.large*4/3,width=24,height=24,shrink=0,color=ink},
            ui.Column{id="device-text",height=40,grow=1,
                ui.Text{id="device-name",text=item.label,height=24,font_size=BODY,color=ink},
                ui.Text{id="device-caption",text=item.caption,height=16,font_size=CAPTION*.85,color=quiet}},
            item.action~="" and kit.Button{id="pair",width=T.spacing.xxl*3+T.spacing.md,height=32,padding=0,padding_top=4,radius=T.radii.small,
                justify="center",align="center",fill="surfaceContainerHigh",aria_label=item.action..", "..item.label,
                on_activate=item.enabled and ui.action("bluetooth.pair",item.id) or nil,
                ui.Text{id="pair-label",text=item.action,height=24,grow=1,font_size=CAPTION,text_align="center",
                    color=item.enabled and "onSurface" or "onSurfaceVariant"}} or nil}
    end},
    message and ui.Text{id="list-message",text=ui.bind("bluetooth.list_message"),font_size=CAPTION,height=20,color="onSurfaceVariant"} or nil,
}
