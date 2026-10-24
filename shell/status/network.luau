-- SPDX-License-Identifier: GPL-3.0-only
-- Network popout: link list (B0017), password prompt, hidden-network form and saved list (B0050)
-- on kit components. `network.view` (list|prompt|hidden|saved) is the flow state the adapter owns;
-- every size comes from the size-scale tokens.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local enabled=get("network.wifi_enabled")
local checked=get("network.wifi_checked")
local view=get("network.view")
local padding=T.spacing.panel
local gap=T.spacing.compact
local width=get("view.width")
local link_inner=width-padding*2-T.spacing.md-T.spacing.sm
local action_width=T.spacing.huge*3
local name_width=link_inner-T.icons.medium*4/3-T.spacing.md*2-action_width
local duration=get("view.duration")
local BODY,CAPTION,TITLE=T.text.body*4/3,T.text.caption*4/3,T.text.title*4/3
local FIELD,BUTTON=40,36
local rows=get("network.rows")
local can_hidden=get("network.can_hidden")
local can_forget=get("network.can_forget")
local can_more=can_hidden or can_forget
local adapter_text=not checked and (not enabled and ui.tr("bead.panel.network-no-adapter") or ui.tr("bead.panel.wifi-off")) or get("network.wifi_message")
local adapter_note=kit.Paragraph{id="wifi-message",text=adapter_text,width=width-padding*2,font_size=BODY,color="onSurfaceVariant"}
local health_text=get("network.health_message")
local health_note=kit.Paragraph{id="network-health",text=health_text,width=width-padding*2,font_size=BODY,color="onSurfaceVariant"}
local empty_sections={ui.Box{id="empty-network-groups",visible=false,height=0}}
if #rows==0 then
    local function empty_section(id,title,detail)
        return ui.Column{id=id,height=38,gap=T.spacing.sm-2,
            ui.Text{id="group-title",text=title,height=18,font_size=CAPTION,font_weight=500,color="onSurfaceVariant"},
            ui.Text{id="group-empty",text=detail,height=18,font_size=(T.text.caption-1)*4/3,color="onSurfaceVariant"}}
    end
    empty_sections={ui.Column{id="empty-network-groups",width=width-padding*2,gap=T.radii.medium,
        empty_section("connected",ui.tr("bead.common.connected"),ui.tr("bead.common.not-connected")),
        empty_section("known",ui.tr("bead.panel.network-known"),ui.tr("bead.panel.network-none-known")),
        empty_section("available",ui.tr("bead.panel.network-available"),ui.tr("bead.panel.network-no-nearby"))}}
end
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
    return kit.ActionButton{id=id,label=label,on_activate=action,primary=primary,height=h}
end
local footer={}
if can_more then
    footer={ui.Row{id="more",height=32,gap=gap,
        button("hidden",ui.tr("bead.panel.network-hidden-open"),can_hidden and ui.action("network.open","hidden") or nil,false,32),
        button("saved",ui.tr("bead.panel.network-saved-title"),can_forget and ui.action("network.open","saved") or nil,false,32)}}
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
                kit.ConfirmButton{id="forget",width=T.spacing.xxl*7,height=BUTTON,padding=T.spacing.sm,radius=T.radii.small,
                    label=ui.tr("bead.panel.network-forget"),confirm_label=ui.tr("bead.panel.network-forget-confirm"),glyph="delete",
                    confirming=item.confirming,
                    on_activate=item.enabled and ui.action("network.forget_ask",item.id) or nil,
                    on_confirm=item.enabled and ui.action("network.forget",item.id) or nil}}
        end},count*FIELD+(count-1)*gap)
    end
    message_line("link-message","network.link_message",get("network.link_message")~="")
    return Page()
end
return kit.Panel{id="status-popout",edge=get("view.edge"),
    open=ui.bind("panel.open"),duration=duration,width=width,
    height=math.max(get("network.height"),get("view.height"),#rows==0 and padding*2+36+adapter_note.props.height+38*3+T.radii.medium*2+health_note.props.height+gap*3 or 0)+(#rows+(get("network.link_visible") and 1 or 0))*(gap-T.spacing.sm)+(can_more and 32+gap or 0),padding=padding,
    gap=gap,radius=T.radii.medium,fill="surface",
    ui.Row{id="wifi-row",width=width-padding*2,height=36,align="center",gap=T.spacing.md,
        ui.Text{id="wifi-label",text=ui.tr("bead.panel.wifi"),height=28,width=width-padding*2-T.spacing.xxl*2-T.spacing.md*2-T.spacing.huge*4,font_size=(T.text.body+1)*4/3,
            color=enabled and "onSurface" or "onSurfaceVariant"},
        kit.Button{id="scan",width=T.spacing.huge*4,height=34,padding=0,radius=T.radii.full,fill=ui.alpha("surfaceContainerHigh",.4),
            ui.Row{width=T.spacing.huge*4,height=34,gap=T.spacing.xs,align="center",justify="center",
                ui.Text{text="refresh",family="Material Symbols Rounded",width=16,height=24,font_size=T.icons.small*4/3,color=ui.alpha("onSurface",.4)},
                ui.Text{text=ui.tr("bead.panel.network-scan"),height=24,font_size=CAPTION,color=ui.alpha("onSurface",.4)}}},
        kit.Switch{id="wifi-radio",checked=ui.bind("network.wifi_checked"),aria_label=ui.tr("bead.panel.wifi-radio"),
            on_activate=enabled and ui.action("network.toggle_wifi") or nil},
    },
    adapter_note,
    empty_sections[1],
    ui.Repeat{id="links",model=ui.bind("network.rows"),key="id",render=function(item)
        local ink=item.active and "onPrimaryContainer" or "onSurface"
        local quiet=item.active and "onPrimaryContainer" or "onSurfaceVariant"
        return kit.Button{id="link",height=56,padding_left=T.spacing.md,padding_right=T.spacing.sm,
            radius=T.radii.medium,fill=item.active and "primaryContainer" or "surfaceContainer",
            aria_label=item.label,
            on_activate=item.enabled and (item.active and ui.action("network.disconnect",item.id) or ui.action("network.connect",item.id)) or nil,
            ui.Row{id="link-row",width=link_inner,height=48,align="center",gap=T.spacing.md,
                ui.Text{id="link-icon",text=item.secured and "wifi_lock" or "wifi",family="Material Symbols Rounded",
                    width=T.icons.medium*4/3,height=24,font_size=T.icons.medium*4/3,color=quiet,shrink=0},
                ui.Text{id="link-name",text=item.label,height=32,width=name_width,font_size=BODY,color=ink},
                ui.Text{id="link-action",text=item.action,height=24,width=action_width,shrink=0,font_size=CAPTION,color=quiet},
            },
        }
    end},
    ui.Text{id="link-message",visible=ui.bind("network.link_visible"),text=ui.bind("network.link_message"),font_size=CAPTION,height=20,color="onSurfaceVariant"},
    health_note,
    table.unpack(footer),
}
