-- SPDX-License-Identifier: GPL-3.0-only
-- Network popout: link list (B0017), password prompt, hidden-network form and saved list (B0050)
-- on kit components. `network.view` (list|prompt|hidden|saved) is the flow state the adapter owns;
-- every size comes from the size-scale tokens.
local ui=require("bead.ui")
local kit=require("bead.kit")
return function(options)
local embedded=options~=nil
local panel=require("./body")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local enabled=get("network.wifi_enabled")
local checked=get("network.wifi_checked")
local view=get("network.view")
local padding=embedded and 0 or T.spacing.panel
local gap=T.spacing.compact
local width=embedded and options.width or get("view.width")
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
local scan_on=get("network.scan_enabled")
local adapter_text=not checked and (not enabled and ui.tr("bead.panel.network-no-adapter") or ui.tr("bead.panel.wifi-off")) or get("network.wifi_message")
local adapter_note=kit.Paragraph{id="wifi-message",text=adapter_text,width=width-padding*2,font_size=BODY,color="onSurfaceVariant"}
local health_text=get("network.health_message")
local health_note=kit.Paragraph{id="network-health",text=health_text,width=width-padding*2,font_size=BODY,color="onSurfaceVariant"}
-- Page children with their heights, so the panel height follows the content and tokens.
local kids,heights={},{}
local function add(node,h)
    table.insert(kids,node);table.insert(heights,h)
end
local function Page()
    local total=2*padding+gap*(#heights-1)
    for _,h in heights do total=total+h end
    return panel(embedded,{id="status-popout",edge=get("view.edge"),
        open=ui.bind("panel.open"),duration=duration,width=width,height=total,padding=padding,
        gap=gap,radius=T.radii.medium,fill="surface",table.unpack(kids)})
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
-- The list view (B0095: laid out as QML's NacreNetworkPopup). QML sizes text by its line box (about 1.295 times
-- the font size); the groups are Connected, Known and Available, always present, each with its rows or a hint.
local LINE=1.295
local ICON=(T.icons.large-1)*4/3
local NAME=(T.text.caption+1)*4/3
local ROWH,LIST_MAX,GROUP_GAP,ROW_GAP=56,210,16,6
local content=width-padding*2
local titles={connected=ui.tr("bead.common.connected"),known=ui.tr("bead.panel.network-known"),available=ui.tr("bead.panel.network-available")}
local hints={connected=ui.tr("bead.common.not-connected"),known=ui.tr("bead.panel.network-none-known"),available=ui.tr("bead.panel.network-no-nearby")}
-- QML ActionButton (compact): a pill of the content plus 18 px, 34 px high; dimmed when it cannot act.
-- Bead lays text out in the box it is given, so the pill is as wide as its label is estimated to be.
local function text_width(text,px) text=tostring(text);return math.ceil((utf8.len(text) or #text)*px*.49) end
local function pill_width(label,glyph) return text_width(label,CAPTION)+(glyph and T.icons.small*4/3+T.spacing.xs or 0)+18 end
local function pill(id,label,glyph,action,armed)
    local ink=armed and "onSurface" or "onSurfaceVariant"
    local inner=pill_width(label,glyph)-18
    if not glyph then
        return kit.Button{id=id,width=inner+18,height=34,padding=0,padding_top=(34-CAPTION*LINE)/2,radius=T.radii.full,shrink=0,
            fill=ui.alpha("surfaceContainerHigh",armed and 1 or .4),aria_label=label,on_activate=armed and action or nil,
            text=label,font_size=CAPTION,text_align="center",color=ink}
    end
    return kit.Button{id=id,width=inner+18,height=34,padding=0,radius=T.radii.full,shrink=0,
        fill=ui.alpha("surfaceContainerHigh",armed and 1 or .4),aria_label=label,on_activate=armed and action or nil,
        ui.Row{id=id.."-row",width=inner+18,height=34,padding_left=9,gap=T.spacing.xs,align="center",
            glyph and ui.Text{id="glyph",text=glyph,family="Material Symbols Rounded",width=T.icons.small*4/3,height=T.icons.small*4/3*LINE,font_size=T.icons.small*4/3,color=ink,shrink=0} or nil,
            ui.Text{id="label",text=label,width=math.ceil(text_width(label,CAPTION)*1.4),height=CAPTION*LINE,font_size=CAPTION,color=ink,shrink=0}}}
end
local function link_card(item)
    local ink=item.active and "onPrimaryContainer" or "onSurface"
    local glyph=item.strength>75 and "signal_wifi_4_bar" or item.strength>45 and "network_wifi_3_bar" or "network_wifi_1_bar"
    local act=item.active and ui.action("network.disconnect",item.id) or ui.action("network.connect",item.id)
    return ui.Row{id="link",width=content,height=ROWH,shrink=0,radius=T.radii.medium,padding_left=T.spacing.md,padding_right=6,gap=5,align="center",
        fill=item.active and "primaryContainer" or "surfaceContainer",
        ui.Text{id="link-icon",text=glyph,family="Material Symbols Rounded",width=25,height=ICON*LINE,font_size=ICON,color=ink,shrink=0},
        ui.Text{id="link-name",text=item.label,height=NAME*LINE,width=content-12-6-25-10-pill_width(item.action),font_size=NAME,color=ink,shrink=0},
        pill(item.active and "connected-action" or "available-action",item.action,nil,act,item.enabled)}
end
local list_children,list_height={},0
for _,g in ipairs({"connected","known","available"}) do
    local cards={}
    for _,item in ipairs(rows) do if item.group==g then cards[#cards+1]=link_card(item) end end
    local body_height=#cards>0 and #cards*ROWH+(#cards-1)*ROW_GAP or CAPTION*.9*LINE
    local box={id="group-"..g,width=content,gap=ROW_GAP,shrink=0,
        ui.Text{id="group-title",text=titles[g],height=CAPTION*LINE,font_size=CAPTION,font_weight=500,color="onSurfaceVariant"}}
    if #cards>0 then for _,card in ipairs(cards) do box[#box+1]=card end
    else box[#box+1]=ui.Text{id="group-empty",text=hints[g],height=CAPTION*.9*LINE,font_size=CAPTION*.9,color="onSurfaceVariant"} end
    list_children[#list_children+1]=ui.Column(box)
    list_height=list_height+CAPTION*LINE+ROW_GAP+body_height
end
list_height=list_height+GROUP_GAP*(#list_children-1)
local groups=ui.Column{id="groups",width=content,gap=GROUP_GAP,shrink=0,table.unpack(list_children)}
local list_node,list_shown=groups,list_height
if not embedded and list_height>LIST_MAX then
    list_node=ui.Scroll{id="links",scroll=ui.bind("network.scroll"),width=content,height=LIST_MAX,groups}
    list_shown=LIST_MAX
end
local head_height=34
local out_kids,out_heights={},{}
local function put(node,h) table.insert(out_kids,node);table.insert(out_heights,h) end
put(ui.Row{id="wifi-row",width=content,height=head_height,align="center",gap=T.spacing.md,
    ui.Text{id="wifi-label",visible=not embedded,text=ui.tr("bead.panel.wifi"),height=(T.text.body+1)*4/3*LINE,grow=1,font_size=(T.text.body+1)*4/3,
        color=enabled and "onSurface" or "onSurfaceVariant"},
    pill("scan",ui.tr("bead.panel.network-scan"),"refresh",ui.action("network.scan"),scan_on),
    ui.Box{id="heading-space",visible=embedded,grow=1,height=1},
    kit.Switch{id="wifi-radio",checked=ui.bind("network.wifi_checked"),aria_label=ui.tr("bead.panel.wifi-radio"),
        on_activate=enabled and ui.action("network.toggle_wifi") or nil}},head_height)
if adapter_text~="" and adapter_text~=nil then put(adapter_note,adapter_note.props.height) end
put(list_node,list_shown)
if #rows==0 and checked then
    put(ui.Text{id="no-nearby",text=ui.tr("bead.panel.network-no-nearby"),width=content,height=NAME*LINE,font_size=NAME,color="onSurfaceVariant"},NAME*LINE)
end
if get("network.link_visible") then
    put(ui.Text{id="link-message",text=ui.bind("network.link_message"),font_size=CAPTION,height=20,color="onSurfaceVariant"},20)
end
if health_text~="" and health_text~=nil then put(health_note,health_note.props.height) end
for _,node in ipairs(footer) do put(node,32) end
local total=2*padding+gap*(#out_heights-1)
for _,h in ipairs(out_heights) do total=total+h end
return panel(embedded,{id="status-popout",edge=get("view.edge"),
    open=ui.bind("panel.open"),duration=duration,width=width,height=total,padding=padding,
    gap=gap,radius=T.radii.medium,fill="surface",table.unpack(out_kids)})
end
