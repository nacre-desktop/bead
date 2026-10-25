-- SPDX-License-Identifier: GPL-3.0-only
-- Settings window: sidebar (search and pages) and a scrolling list of form rows. The rows are
-- plain data from the adapter; this file only maps a row kind to a kit component.
local ui=require("bead.ui")
local kit=require("bead.kit")
local sp=kit.tokens.spacing
local tx=kit.tokens.text
local writable=ui.resolve_for_check(ui.bind("settings.writable"))
local searching=ui.resolve_for_check(ui.bind("settings.searching"))
local opened=ui.resolve_for_check(ui.bind("settings.select"))
local function act(arg) return ui.action("settings.do",arg) end
local function preview_clip(w,h)
    local radius=math.min(kit.tokens.radii.medium,w/2,h/2)
    local rx=radius/math.max(1,w)
    local ry=radius/math.max(1,h)
    local points={}
    for corner,center in ipairs{{1-rx,ry},{1-rx,1-ry},{rx,1-ry},{rx,ry}} do
        for step=0,7 do
            local angle=(corner-2)*math.pi/2+step*math.pi/14
            points[#points+1]={math.clamp(center[1]+rx*math.cos(angle),0,1),math.clamp(center[2]+ry*math.sin(angle),0,1)}
        end
    end
    return points
end
-- B0067: the QML rail. Width huge*7+xl, group captions huge+hairline tall, items huge+compact tall with a caption-size
-- label and an accent wash on the current page (SettingsView.qml).
local RAIL=sp.huge*7+sp.xl
local function nav(item)
    if item.kind=="group" then
        return ui.Text{id="group",text=item.label,height=sp.huge+sp.hairline,padding_left=sp.md,padding_top=sp.md,font_size=tx.caption*4/3,color="onSurfaceVariant"}
    end
    return ui.Button{id="page",width="fill",height=sp.huge+sp.compact,radius=kit.tokens.radii.small,padding=0,color="onSurface",
        selected=item.selected,fill="transparent",selected_fill=ui.alpha("primary",.18),
        hover_fill=ui.alpha("onSurface",.06),aria_label=item.label,on_activate=act("page:"..item.id),
        ui.Row{grow=1,gap=sp.compact,align="center",padding_left=sp.md,
            ui.Text{id="icon",text=item.icon,family="Material Symbols Rounded",font_size=kit.tokens.icons.large*4/3,width=24,height=28},
            ui.Text{id="label",text=item.label,font_size=tx.caption*4/3,grow=1,height=18}}}
end
local row
local function section(item)
    local padding=item.id=="section:wallpaper.scene" and sp.md or sp.xl
    local children={id="section",width="fill",shrink=0,padding=padding,gap=sp.xs,radius=kit.tokens.radii.medium,fill="surfaceContainer"}
    children[1]=ui.Text{id="title",text=item.label,height=24,font_size=tx.title*4/3,color="onSurface"}
    children[2]=ui.Text{id="detail",text=item.description or "",visible=(item.description or "")~="",height=16,font_size=tx.caption*4/3,color="onSurfaceVariant"}
    -- Kit rows use fixed child ids, so each row sits in its own uniquely named wrapper.
    for i,child in ipairs(item.rows or {}) do
        table.insert(children,ui.Column{id="r"..i,width="fill",shrink=0,row(child)})
    end
    return ui.Column(children)
end
-- A setting the shell cannot offer: its label and description stay, with the reason as a muted caption (QML SettingsRow).
local function unavailable(item)
    return ui.Column{id="unavailable",width="fill",shrink=0,gap=sp.hairline,padding_left=sp.sm,padding_top=sp.sm,padding_bottom=sp.sm,
        ui.Text{id="label",text=item.title,height=20,font_size=tx.body*4/3,color="onSurfaceVariant"},
        ui.Text{id="description",text=item.description or "",visible=(item.description or "")~="",height=16,font_size=tx.caption*4/3,color="onSurfaceVariant"},
        ui.Text{id="note",text=item.note,height=16,font_size=tx.caption*4/3,color="onSurfaceVariant"}}
end
function row(item)
    local on=item.enabled and (writable or item.own)
    local id=item.id
    local k=item.kind
    local node
    if k=="section" then return section(item)
    elseif k=="help" and item.note then return unavailable(item)
    elseif k=="help" then return kit.HelpText{id="help",text=item.label}
    elseif k=="link" then
        return ui.Button{id="link",width="fill",height=56,shrink=0,radius=kit.tokens.radii.small,padding_left=sp.md,
            fill="surfaceContainer",hover_fill="surfaceContainerHigh",aria_label=item.label,on_activate=act("page:"..item.page),
            ui.Column{grow=1,justify="center",gap=sp.hairline,
                ui.Text{id="label",text=item.label,height=20,font_size=tx.body*4/3,color="onSurface"},
                ui.Text{id="detail",text=item.description,height=16,font_size=tx.caption*4/3,color="onSurfaceVariant"}}}
    elseif k=="info" then
        return kit.InfoRow{id="info",label=item.label,description=item.description,value=item.value}
    elseif k=="wallpaper-scene" then
        local viewport=ui.resolve_for_check(ui.bind("bead.viewport.width")) or ui.resolve_for_check(ui.bind("window.width"))
        local preview_w=math.max(1,viewport-2*sp.lg-RAIL-sp.xxl-2*sp.md)
        local preview_h=math.min(preview_w*9/16,sp.huge*12)
        local label_w=math.ceil(ui.text_width(item.open_label,"IBM Plex Sans",tx.body*4/3))+sp.hairline
        local icon_w=kit.tokens.icons.medium*4/3
        local picker_w=sp.huge+icon_w+sp.sm+label_w
        local picker_h=math.max(sp.huge+sp.hairline,math.ceil(tx.body*4/3*1.35)+sp.md)
        local preview=item.preview~="" and ui.Image{id="preview",width="fill",height=preview_h,shrink=0,
            icon=item.preview,image_fit="cover",clip_polygon=preview_clip(preview_w,preview_h),aria_label=item.name} or
            ui.Box{id="preview-empty",width="fill",height=preview_h,shrink=0,radius=kit.tokens.radii.medium,
                fill="surfaceContainerHigh",
                ui.Text{id="preview-icon",width="fill",height="fill",text="image",family="Material Symbols Rounded",
                    font_size=kit.tokens.icons.large*2,color="onSurfaceVariant",text_align="center"}}
        return ui.Column{id="wallpaper-scene",width="fill",shrink=0,gap=sp.sm,
            preview,
            ui.Row{id="scene-details",width="fill",height=sp.huge+sp.sm,align="center",gap=sp.md,
                ui.Column{id="scene-text",grow=1,justify="center",gap=sp.hairline,
                    ui.Text{id="scene-name",text=item.name,width="fill",height=20,font_size=tx.body*4/3,color="onSurface"},
                    ui.Text{id="scene-kind",text=item.scene_kind,width="fill",height=16,font_size=tx.caption*4/3,color="onSurfaceVariant"}},
                kit.Button{id="open-picker",width=picker_w,shrink=0,height=picker_h,padding_left=sp.huge/2,padding_right=sp.huge/2,
                    radius=kit.tokens.radii.full,fill="surfaceContainerHigh",hover_fill="surfaceContainerHighest",
                    aria_label="Open wallpaper picker",activate_on_release=true,on_activate=on and act("do:open|wallpaper") or nil,
                    ui.Row{width=picker_w-sp.huge,height="fill",align="center",gap=sp.sm,
                        ui.Text{id="picker-icon",text="wallpaper",family="Material Symbols Rounded",
                            width=icon_w,height=icon_w,font_size=icon_w,color="onSurface"},
                        ui.Text{id="picker-label",text=item.open_label,width=label_w,shrink=0,height=22,font_size=tx.body*4/3,color="onSurface"}}}}
        }
    elseif k=="actions" then
        if #item.options>3 or item.chips then
            -- A row that is off offers dim chips (the kit builds every chip's action, so the name stays set).
            -- A `chips` row sizes its chips by label (long labels such as display modes need 10 px a character).
            local chips=item.options
            if item.chips or not on then
                chips={}
                for i,o in ipairs(item.options) do
                    chips[i]={id=o.id,label=o.label,selected=o.selected,enabled=on and o.enabled~=false,
                        width=item.chips and 2*sp.md+8+(utf8.len(o.label) or 4)*10 or nil}
                end
            end
            return kit.ChipsRow{id="chips",label=item.label,description=item.description,enabled=on,items=chips,per_row=item.per_row,on_change="settings.do"}
        end
        node=kit.ChoiceRow{id="actions",label=item.label,description=item.description,enabled=on,options=item.options,on_change="settings.do"}
    elseif k=="device" then
        return kit.DeviceRow{id="device",title=item.title,detail=item.detail,icon=item.icon,enabled=on,buttons=item.buttons,slider=item.slider,on_action="settings.do"}
    elseif k=="picker" then
        return kit.PickerList{id="picker",label=item.label,placeholder=item.placeholder,enabled=on,items=item.items,empty=item.empty,more=item.more,
            field=ui.bind("settings.pick.text"),on_pick="settings.do"}
    elseif k=="secret" then
        return kit.SecretRow{id="secret",label=item.label,enabled=on,field=ui.bind("settings.secret"),revealed=ui.bind("settings.reveal"),
            on_toggle=act("do:reveal"),on_join=act(item.join),on_cancel=act(item.cancel),join_label=item.join_label,cancel_label=item.cancel_label,
            show_label=ui.tr("bead.panel.network-password-show"),hide_label=ui.tr("bead.panel.network-password-hide")}
    elseif k=="toggle" then
        node=kit.ToggleRow{id="toggle",label=item.label,description=item.description,checked=item.checked,enabled=on,on_activate=act("set:"..id)}
    elseif k=="choice" then
        node=kit.ChoiceRow{id="choice",label=item.label,description=item.description,enabled=on,options=item.options,on_change="settings.do"}
    elseif k=="select" then
        -- A drop-down (QML NacreSelect): a field with the current choice; the list opens under it. `filter` rows
        -- (many options) carry the page's shared search field and only the matching options.
        node=kit.SelectRow{id="select",label=item.label,description=item.description,enabled=on,text=item.text,options=item.options,
            width=sp.huge*(item.wide and 10 or 8),open=opened==id,on_toggle=ui.action("settings.dropdown",id),on_change="settings.do",
            scroll=ui.bind("settings.select.scroll"),field=item.filter and ui.bind("settings.pick.text") or nil,
            placeholder=item.filter,empty=item.empty,more=item.more}
    elseif k=="number" then
        local n=item.number
        node=kit.NumberRow{id="number",label=item.label,description=item.description,enabled=on,text=item.text,unit=item.unit~="" and " "..item.unit or "",
            at_min=item.at_min,at_max=item.at_max,
            on_decrease=act("set:"..id.."|"..(n-item.step)),on_increase=act("set:"..id.."|"..(n+item.step))}
    elseif k=="text" then
        node=kit.TextRow{id="text",label=item.label,description=item.description,enabled=on,text=item.text,swatch=item.swatch,
            editing=item.editing,field=ui.bind("settings.edit.text"),on_edit=act("edit:"..id),on_save=act("set:"..id),on_cancel=act("cancel"),
            save_label=ui.tr("bead.settings.save"),cancel_label=ui.tr("bead.common.cancel")}
    elseif k=="list" then
        node=kit.ListEditor{id="list",label=item.label,enabled=on,items=item.items,editing=item.editing,field=ui.bind("settings.edit.text"),
            on_edit=act("edit:"..id),on_save=act("set:"..id.."|add"),on_cancel=act("cancel"),on_remove="settings.do",remove_prefix="set:",reorder=item.reorder,up_label=ui.tr("bead.settings.move-up"),down_label=ui.tr("bead.settings.move-down"),
            add_label=ui.tr("bead.settings.add"),remove_label=ui.tr("bead.settings.remove"),
            save_label=ui.tr("bead.settings.save"),cancel_label=ui.tr("bead.common.cancel")}
    else
        return kit.HelpText{id="help",text=item.label}
    end
    if item.staged then
        return ui.Column{id="staged",width="fill",shrink=0,gap=sp.xs,node,
            kit.ApplyBar{id="apply-bar",text=ui.tr("bead.settings.requires-apply"),apply_label=ui.tr("bead.settings.apply"),
                discard_label=ui.tr("bead.settings.discard"),on_apply=act("apply:"..id),on_discard=act("discard")}}
    end
    return node
end
-- B0067: the QML window: a 64 px header (logo, window title, current page at the right) over a rule, then a raised
-- rail and the page with section cards, inset by lg.
local function header()
    return ui.Column{id="header-box",width="fill",height=65,shrink=0,
        ui.Row{id="header",width="fill",height=64,shrink=0,align="center",gap=sp.lg,padding_left=sp.xl,padding_right=sp.xl,
            ui.Image{id="logo",path="settings/assets/nacre-logo.themed.svg",width=kit.tokens.icons.hero,height=kit.tokens.icons.hero,shrink=0,aria_label=ui.tr("bead.settings.title")},
            ui.Text{id="window-title",text=ui.tr("bead.settings.title"),height=28,grow=1,font_size=tx.title*4/3,color="onSurface"},
            ui.Text{id="current",text=ui.bind("settings.title"),height=20,font_size=tx.caption*4/3,color="onSurfaceVariant"}},
        ui.Box{id="rule",width="fill",height=1,fill="outlineVariant"}}
end
return ui.Column{id="settings",width="100%",height="100%",fill="surface",
    header(),
    ui.Row{id="body",width="fill",grow=1,padding=sp.lg,gap=sp.xxl,
    ui.Column{id="sidebar",width=RAIL,height="100%",shrink=0,padding=sp.md,gap=sp.sm,radius=kit.tokens.radii.medium,fill="surfaceContainer",
        kit.TextInput{id="search",text=ui.bind("settings.search"),placeholder=ui.tr("bead.settings.search-placeholder"),
            height=sp.huge+sp.md,fill="surface",border_width=1,border_color="outlineVariant",
            width="fill",aria_label=ui.tr("bead.settings.search-placeholder")},
        ui.Scroll{id="nav-scroll",scroll=ui.bind("settings.nav.scroll"),grow=1,
            ui.Column{id="nav-list",width="fill",gap=sp.xs,
                ui.Repeat{id="nav",model=ui.bind("settings.nav"),key="id",render=nav}}}},
    ui.Column{id="content",grow=1,height="100%",gap=sp.sm,
        ui.Text{id="title",text=ui.bind("settings.title"),height=32,font_size=tx.headline*4/3,color="onSurface"},
        ui.Text{id="description",text=ui.bind("settings.description"),visible=not searching,height=20,font_size=tx.body*4/3,color="onSurfaceVariant"},
        ui.Text{id="readonly",text=ui.tr("bead.settings.read-only"),visible=ui.bind("settings.readonly"),height=20,font_size=tx.caption*4/3,color="onSurfaceVariant"},
        ui.Text{id="message",text=ui.bind("settings.message"),height=20,font_size=tx.caption*4/3,color="error"},
        ui.Text{id="notice",text=ui.bind("settings.notice"),visible=ui.bind("settings.noticed"),height=20,font_size=tx.caption*4/3,color="primary"},
        ui.Text{id="empty",text=ui.tr("bead.settings.no-results"),visible=ui.bind("settings.empty"),height=20,font_size=tx.body*4/3,color="onSurfaceVariant"},
        ui.Scroll{id="items-scroll",scroll=ui.bind("settings.scroll"),grow=1,
            ui.Column{id="items",width="fill",gap=searching and sp.xs or sp.xl,
                ui.Repeat{id="rows",model=ui.bind("settings.items"),key="id",render=row}}}}}}
