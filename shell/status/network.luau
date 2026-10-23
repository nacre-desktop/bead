-- SPDX-License-Identifier: GPL-3.0-only
-- Network popout: link list (B0017), password prompt, hidden-network form and saved list (B0050)
-- on kit components. `network.view` (list|prompt|hidden|saved) is the flow state the adapter owns;
-- every size comes from the size-scale tokens.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local ready=get("status.ready")
local active=get("status.active")
local enabled=get("network.wifi_enabled")
local view=get("network.view")
local padding=T.spacing.md
local gap=T.spacing.sm
local icon_size=T.icons.xlarge*4/3
local width=get("view.width")
local duration=get("view.duration")
local BODY,CAPTION,TITLE=T.text.body*4/3,T.text.caption*4/3,T.text.title*4/3
local FIELD,BUTTON=40,36
-- Page children with their heights, so the panel height follows the content and tokens.
local kids,heights={},{}
local function add(node,h)
    table.insert(kids,node);table.insert(heights,h)
end
local function Page()
    local total=2*padding+gap*(#heights-1)
    for _,h in heights do total=total+h end
    return kit.Panel{id="status-popout",edge=get("view.edge"),
        open=ui.bind("panel.open"),duration=duration,width=width,height=total,padding=padding,
        gap=gap,radius=T.radii.medium,fill="surface",table.unpack(kids)}
end
local function button(id,label,action,primary,h)
    return kit.Button{id=id,height=h or BUTTON,grow=1,padding=0,radius=T.radii.small,justify="center",align="center",
        fill=primary and (action and "primary" or "surfaceContainerHigh") or "surfaceContainer",aria_label=label,on_activate=action,
        ui.Text{id=id.."-label",text=label,height=24,grow=1,font_size=BODY,text_align="center",
            color=primary and action and "onPrimary" or (action and "onSurface" or "onSurfaceVariant")}}
end
-- Back arrow plus title: leaves the prompt or form without sending anything.
local function header(title)
    add(ui.Row{id="head",height=FIELD,gap=gap,align="center",
        kit.IconButton{id="back",text="arrow_back",aria_label=ui.tr("bead.panel.network-back"),on_activate=ui.action("network.cancel")},
        ui.Text{id="title",text=title,height=24,grow=1,font_size=TITLE,color="onSurface"}},FIELD)
end
local function password(busy)
    add(kit.PasswordField{id="secret",height=FIELD,text=ui.bind("network.secret"),revealed=ui.bind("network.reveal"),
        placeholder=ui.tr("bead.panel.network-password"),aria_label=ui.tr("bead.panel.network-password"),
        show_label=ui.tr("bead.panel.network-password-show"),hide_label=ui.tr("bead.panel.network-password-hide"),
        on_toggle=ui.action("network.toggle_reveal"),on_activate=(not busy) and ui.action("network.join") or nil},FIELD)
end
local function message_line(id,name,visible)
    if visible then
        add(ui.Text{id=id,text=ui.bind(name),height=20,font_size=CAPTION,color="error"},20)
    end
end
if view=="prompt" or view=="hidden" then
    local busy=get("network.form_busy")
    header(view=="prompt" and ui.bind("network.prompt_label") or ui.tr("bead.panel.network-hidden-title"))
    if view=="prompt" then
        add(ui.Text{id="hint",text=ui.tr("bead.panel.network-prompt-hint"),height=20,font_size=CAPTION,color="onSurfaceVariant"},20)
    else
        add(kit.TextInput{id="ssid",height=FIELD,text=ui.bind("network.hidden_ssid"),
            placeholder=ui.tr("bead.panel.network-ssid"),aria_label=ui.tr("bead.panel.network-ssid")},FIELD)
        add(kit.SegmentedControl{id="security",height=FIELD,value=ui.bind("network.hidden_security"),on_change="network.set_security",
            items={{id="open",label=ui.tr("bead.panel.network-security-open")},{id="wpa_psk",label=ui.tr("bead.panel.network-security-wpa")},
                {id="sae",label=ui.tr("bead.panel.network-security-wpa3")}}},FIELD)
    end
    if view=="prompt" or get("network.hidden_security")~="open" then password(busy) end
    message_line("form-error","network.form_error",get("network.form_error")~="")
    add(ui.Row{id="actions",height=BUTTON,gap=gap,
        button("cancel",ui.tr("bead.panel.network-cancel"),ui.action("network.cancel")),
        button("join",view=="prompt" and ui.tr("bead.panel.network-connect") or ui.tr("bead.panel.network-join"),
            (not busy) and ui.action("network.join") or nil,true)},BUTTON)
    return Page()
end
if view=="saved" then
    local count=#get("network.saved")
    header(ui.tr("bead.panel.network-saved-title"))
    if count==0 then
        add(ui.Text{id="empty",text=ui.tr("bead.panel.network-saved-none"),height=24,font_size=BODY,color="onSurfaceVariant"},24)
    else
        add(ui.Repeat{id="saved",model=ui.bind("network.saved"),key="id",render=function(item)
            return ui.Row{id="saved-row",height=FIELD,gap=gap,align="center",
                ui.Column{id="saved-text",height=FIELD,grow=1,
                    ui.Text{id="saved-name",text=item.label,height=24,font_size=BODY,color="onSurface"},
                    ui.Text{id="saved-caption",text=item.caption,height=16,font_size=CAPTION*.85,color="onSurfaceVariant"}},
                kit.ConfirmButton{id="forget",width=T.spacing.xxl*5,height=BUTTON,padding=T.spacing.sm,radius=T.radii.small,
                    label=ui.tr("bead.panel.network-forget"),confirm_label=ui.tr("bead.panel.network-forget-confirm"),glyph="delete",
                    confirming=item.confirming,
                    on_activate=item.enabled and ui.action("network.forget_ask",item.id) or nil,
                    on_confirm=item.enabled and ui.action("network.forget",item.id) or nil}}
        end},count*FIELD+(count-1)*gap)
    end
    message_line("link-message","network.link_message",get("network.link_message")~="")
    return Page()
end
local text_width=width-padding*2-icon_size-T.spacing.md
return kit.Panel{id="status-popout",edge=get("view.edge"),
    open=ui.bind("panel.open"),duration=duration,width=width,height=math.max(get("network.height"),get("view.height")),padding=padding,
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
    ui.Text{id="detail",text=ui.bind("status.detail"),font_size=CAPTION,height=24,color="onSurfaceVariant"},
    ui.Row{id="wifi-row",height=36,align="center",gap=T.spacing.md,
        ui.Text{id="wifi-label",text=ui.tr("bead.panel.wifi"),height=28,grow=1,font_size=BODY,
            color=enabled and "onSurface" or "onSurfaceVariant"},
        kit.Switch{id="wifi-radio",checked=ui.bind("network.wifi_checked"),aria_label=ui.tr("bead.panel.wifi-radio"),
            on_activate=enabled and ui.action("network.toggle_wifi") or nil},
    },
    ui.Text{id="wifi-message",text=ui.bind("network.wifi_message"),font_size=CAPTION,height=20,color="onSurfaceVariant"},
    ui.Repeat{id="links",model=ui.bind("network.rows"),key="id",render=function(item)
        return kit.Button{id="link",height=32,padding=T.spacing.sm,fill="transparent",
            aria_label=item.label,
            on_activate=item.enabled and (item.active and ui.action("network.disconnect",item.id) or ui.action("network.connect",item.id)) or nil,
            ui.Row{id="link-row",height=28,align="center",gap=T.spacing.md,
                ui.Text{id="link-name",text=item.label,height=24,grow=1,font_size=BODY,color=item.enabled and "onSurface" or "onSurfaceVariant"},
                item.secured and ui.Text{id="link-lock",text="lock",family="Material Symbols Rounded",font_size=CAPTION,width=CAPTION,height=24,shrink=0,color="onSurfaceVariant"} or nil,
                ui.Text{id="link-action",text=item.action,height=24,font_size=CAPTION,color=item.active and "primary" or "onSurfaceVariant"},
            },
        }
    end},
    ui.Text{id="link-message",visible=ui.bind("network.link_visible"),text=ui.bind("network.link_message"),font_size=CAPTION,height=20,color="onSurfaceVariant"},
    ui.Row{id="more",height=32,gap=gap,
        button("hidden",ui.tr("bead.panel.network-hidden-open"),get("network.can_hidden") and ui.action("network.open","hidden") or nil,false,32),
        button("saved",ui.tr("bead.panel.network-saved-title"),get("network.can_forget") and ui.action("network.open","saved") or nil,false,32)},
}
