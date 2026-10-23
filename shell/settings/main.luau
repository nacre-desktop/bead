-- SPDX-License-Identifier: GPL-3.0-only
-- Settings window: sidebar (search and pages) and a scrolling list of form rows. The rows are
-- plain data from the adapter; this file only maps a row kind to a kit component.
local ui=require("bead.ui")
local kit=require("bead.kit")
local sp=kit.tokens.spacing
local tx=kit.tokens.text
local writable=ui.resolve_for_check(ui.bind("settings.writable"))
local searching=ui.resolve_for_check(ui.bind("settings.searching"))
local function act(arg) return ui.action("settings.do",arg) end
local function nav(item)
    if item.kind=="group" then
        return ui.Text{id="group",text=item.label,height=28,font_size=tx.caption*4/3,color="onSurfaceVariant"}
    end
    return ui.Button{id="page",width="fill",height=40,radius=kit.tokens.radii.small,padding=0,color="onSurface",
        selected=item.selected,fill="transparent",selected_fill=ui.tint("surface","onSurface",.12),
        hover_fill=ui.tint("surface","onSurface",.04),aria_label=item.label,on_activate=act("page:"..item.id),
        ui.Row{grow=1,gap=sp.compact,align="center",padding_left=sp.md,
            ui.Text{id="icon",text=item.icon,family="Material Symbols Rounded",font_size=kit.tokens.icons.large*4/3,width=24,height=28},
            ui.Text{id="label",text=item.label,font_size=tx.body*4/3,grow=1,height=24}}}
end
local function row(item)
    local on=item.enabled and writable
    local id=item.id
    local k=item.kind
    local node
    if k=="section" then return kit.SectionHeader{id="section",label=item.label,description=item.description}
    elseif k=="help" then return kit.HelpText{id="help",text=item.label}
    elseif k=="link" then
        return ui.Button{id="link",width="fill",height=56,shrink=0,radius=kit.tokens.radii.small,padding_left=sp.md,
            fill="surfaceContainer",hover_fill="surfaceContainerHigh",aria_label=item.label,on_activate=act("page:"..item.page),
            ui.Column{grow=1,justify="center",gap=sp.hairline,
                ui.Text{id="label",text=item.label,height=20,font_size=tx.body*4/3,color="onSurface"},
                ui.Text{id="detail",text=item.description,height=16,font_size=tx.caption*4/3,color="onSurfaceVariant"}}}
    elseif k=="toggle" then
        node=kit.ToggleRow{id="toggle",label=item.label,description=item.description,checked=item.checked,enabled=on,on_activate=act("set:"..id)}
    elseif k=="choice" then
        node=kit.ChoiceRow{id="choice",label=item.label,description=item.description,enabled=on,options=item.options,on_change="settings.do"}
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
return ui.Row{id="settings",width="100%",height="100%",fill="surface",
    ui.Column{id="sidebar",width=240,height="100%",padding=sp.md,gap=sp.sm,fill="surfaceContainerLow",
        kit.TextInput{id="search",text=ui.bind("settings.search"),placeholder=ui.tr("bead.settings.search-placeholder"),
            width="fill",aria_label=ui.tr("bead.settings.search-placeholder")},
        ui.Scroll{id="nav-scroll",scroll=ui.bind("settings.nav.scroll"),grow=1,
            ui.Column{id="nav-list",width="fill",gap=sp.hairline,
                ui.Repeat{id="nav",model=ui.bind("settings.nav"),key="id",render=nav}}}},
    ui.Column{id="content",grow=1,height="100%",padding=sp.xl,gap=sp.sm,
        ui.Text{id="title",text=ui.bind("settings.title"),height=32,font_size=tx.headline*4/3,color="onSurface"},
        ui.Text{id="description",text=ui.bind("settings.description"),visible=not searching,height=20,font_size=tx.body*4/3,color="onSurfaceVariant"},
        ui.Text{id="readonly",text=ui.tr("bead.settings.read-only"),visible=ui.bind("settings.readonly"),height=20,font_size=tx.caption*4/3,color="onSurfaceVariant"},
        ui.Text{id="message",text=ui.bind("settings.message"),height=20,font_size=tx.caption*4/3,color="error"},
        ui.Text{id="empty",text=ui.tr("bead.settings.no-results"),visible=ui.bind("settings.empty"),height=20,font_size=tx.body*4/3,color="onSurfaceVariant"},
        ui.Scroll{id="items-scroll",scroll=ui.bind("settings.scroll"),grow=1,
            ui.Column{id="items",width="fill",gap=sp.xs,
                ui.Repeat{id="rows",model=ui.bind("settings.items"),key="id",render=row}}}}}
