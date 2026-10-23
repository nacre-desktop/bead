-- SPDX-License-Identifier: GPL-3.0-only
-- Bluetooth popout on kit components: heading with Find devices and the power switch (B0018,
-- B0062), device cards with Pair/Connect/Disconnect (B0055, B0062) and the pairing page (B0055). `bluetooth.view` (list|pair) is the flow state the adapter owns and
-- `bluetooth.pair_kind` selects the pairing prompt; every size comes from the size-scale tokens.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local visible=get("bluetooth.power_visible")
local enabled=get("bluetooth.power_enabled")
local view=get("bluetooth.view")
local padding=T.spacing.md
local gap=T.spacing.sm
local width=get("view.width")
local duration=get("view.duration")
local BODY,CAPTION,TITLE=T.text.body*4/3,T.text.caption*4/3,T.text.title*4/3
local FIELD,ROW=40,68
if view=="pair" then
    -- Page children with their heights, so the panel height follows the content and tokens.
    local kids,heights={},{}
    local function add(node,h)
        table.insert(kids,node);table.insert(heights,h)
    end
    local kind=get("bluetooth.pair_kind")
    local busy=get("bluetooth.pair_busy")
    add(ui.Text{id="title",text=ui.bind("bluetooth.pair_title"),height=24,font_size=TITLE,color="onSurface"},24)
    -- Wrapped text needs its lines up front: about 0.47 em per character of the caption size.
    local per_line=math.floor((width-padding*2)/(CAPTION*.466))
    local lines=math.min(3,math.max(1,math.ceil((utf8.len(get("bluetooth.pair_text")) or 0)/per_line)))
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
-- List page, as the QML popup: heading with Find devices and the power switch, one card per
-- device (glyph, name, battery or state, Pair/Connect/Disconnect), then the empty or error text.
local kids,heights={},{}
local function add(node,h) table.insert(kids,node);table.insert(heights,h) end
local count=#get("bluetooth.devices")
local message=get("bluetooth.list_message")~=""
local scanning=get("bluetooth.scanning")
local scan_on=get("bluetooth.scan_enabled")
local inner=width-padding*2
local scan_w,switch_w=T.spacing.huge*4,48
-- The heading takes what the buttons leave (a growing text would push them out of the panel).
local head_w=inner-(visible and scan_w+switch_w+gap*2 or 0)
local head={id="bluetooth-row",height=36,align="center",gap=T.spacing.sm,
    ui.Text{id="heading",text=ui.tr("bead.panel.bluetooth"),height=28,width=head_w,font_size=TITLE,
        color=(visible and not enabled) and "onSurfaceVariant" or "onSurface"}}
if visible then
    local inner_w=scan_w-T.spacing.sm*2
    table.insert(head,kit.Button{id="scan",width=scan_w,height=32,padding=0,padding_top=4,radius=T.radii.small,justify="center",align="center",
        fill="surfaceContainerHigh",aria_label=ui.bind("bluetooth.scan_label"),on_activate=scan_on and ui.action("bluetooth.scan") or nil,
        ui.Row{id="scan-row",width=inner_w,height=24,gap=T.spacing.xs,align="center",justify="center",
            ui.Text{id="scan-icon",text=scanning and "bluetooth_searching" or "search",family="Material Symbols Rounded",
                font_size=T.icons.medium*4/3,width=T.icons.medium*4/3,height=24,shrink=0,color=scanning and "primary" or (scan_on and "onSurface" or "onSurfaceVariant")},
            ui.Text{id="scan-label",text=ui.bind("bluetooth.scan_label"),height=24,width=inner_w-T.icons.medium*4/3-T.spacing.xs,text_align="center",font_size=CAPTION,
                color=scan_on and "onSurface" or "onSurfaceVariant"}}})
    table.insert(head,kit.Switch{id="bluetooth-power",checked=ui.bind("bluetooth.power_checked"),aria_label=ui.tr("bead.panel.bluetooth-radio"),
        on_activate=enabled and ui.action("bluetooth.toggle_power") or nil})
end
add(ui.Row(head),36)
if visible and get("bluetooth.power_message")~="" then
    add(ui.Text{id="bluetooth-message",text=ui.bind("bluetooth.power_message"),font_size=CAPTION,height=20,color="onSurfaceVariant"},20)
end
local button_w=T.spacing.huge*3+T.spacing.sm
-- A name that does not fit its column is cut with an ellipsis (about half the font size per character).
local name_chars=math.floor((inner-T.spacing.md-T.spacing.sm-24-T.spacing.md*2-button_w)/(BODY/2))
local function fit(name)
    if (utf8.len(name) or 0)<=name_chars then return name end
    return string.sub(name,1,(utf8.offset(name,name_chars) or #name+1)-1).."…"
end
if count>0 then add(ui.Repeat{id="devices",model=ui.bind("bluetooth.devices"),key="id",render=function(item)
    local ink=item.active and "onPrimaryContainer" or "onSurface"
    local quiet=item.active and "onPrimaryContainer" or "onSurfaceVariant"
    local act=item.enabled and ui.action(item.kind=="pair" and "bluetooth.pair" or "bluetooth.link",item.id) or nil
    local state={id="device-state",height=20,gap=T.spacing.xs,align="center"}
    if item.battery~="" then
        table.insert(state,ui.Text{id="device-battery",text=item.battery,family="Material Symbols Rounded",font_size=T.icons.small*4/3,
            width=T.icons.small*4/3,height=16,shrink=0,color=quiet})
    end
    table.insert(state,ui.Text{id="device-caption",text=item.caption,height=16,font_size=CAPTION*.9,color=quiet})
    return ui.Row{id="device",height=ROW,padding_left=T.spacing.md,padding_right=T.spacing.sm,gap=T.spacing.md,align="center",
        radius=T.spacing.md,fill=item.active and "primaryContainer" or "surfaceContainer",
        ui.Text{id="device-icon",text=item.icon,family="Material Symbols Rounded",font_size=T.icons.large*4/3,width=24,height=24,shrink=0,color=ink},
        ui.Column{id="device-text",height=44,grow=1,
            ui.Text{id="device-name",text=fit(item.label),height=24,font_size=BODY,color=ink},
            ui.Row(state)},
        kit.Button{id="act",width=button_w,height=32,padding=0,padding_top=4,radius=T.radii.small,
            justify="center",align="center",fill="surfaceContainerHigh",aria_label=item.action..", "..item.label,on_activate=act,
            ui.Text{id="act-label",text=item.action,height=24,grow=1,font_size=CAPTION,text_align="center",
                color=item.enabled and "onSurface" or "onSurfaceVariant"}}}
end},count*ROW+(count-1)*gap) end
if get("bluetooth.empty_text")~="" then
    add(ui.Text{id="empty",text=ui.bind("bluetooth.empty_text"),font_size=BODY,height=24,color="onSurfaceVariant"},24)
end
if message then
    add(ui.Text{id="list-message",text=ui.bind("bluetooth.list_message"),font_size=CAPTION,height=20,color="onSurfaceVariant"},20)
end
local total=2*padding+gap*(#heights-1)
for _,h in heights do total=total+h end
return kit.Panel{id="status-popout",edge=get("view.edge"),
    open=ui.bind("panel.open"),duration=duration,width=width,height=total,padding=padding,
    gap=gap,radius=T.radii.medium,fill="surface",table.unpack(kids)}
