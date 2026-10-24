-- SPDX-License-Identifier: GPL-3.0-only
-- Control center (B0049): the QML right-hand sidebar as Luau over kit components. Tiles come from
-- `tiles.luau`; the host projects only data. Sizes are multiples of the shared scale.
local ui=require("bead.ui")
local kit=require("bead.kit")
local reg=require("./tiles")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local W,PAD,GAP=reg.layout.width,T.spacing.xl,T.spacing.sm
local INNER=W-2*PAD
local BODY,CAPTION=T.text.body*4/3,T.text.caption*4/3
local HEAD=T.spacing.huge
local BATTERY_W=T.spacing.huge+T.spacing.md -- the percentage; the estimate takes the rest of the row
local ROW=T.spacing.huge+T.spacing.md
local page=get("cc.page")
local src,views={},{}
for _,s in ipairs(get("cc.src")) do src[s.id]=s end
for _,v in ipairs(get("cc.views")) do views[v.id]=true end
local function usable(t)
    if t.source then
        local s=src[t.source]
        return s~=nil and s.available==true and (not t.need or s[t.need]==true)
    end
    if t.view then return views[t.view]==true end
    return t.page~=nil
end
local function supported(t)
    local s=t.source and src[t.source]
    return not t.hide_unsupported or (s~=nil and s.available==true)
end
local defaults={}
for _,t in ipairs(reg.tiles) do if t.default then table.insert(defaults,t.id) end end
local order
local function ids(list) local out={} for i,item in ipairs(list) do out[i]=item.id end return out end
if get("cc.edited") then order=ids(get("cc.order")) elseif get("cc.saved_known") then order=ids(get("cc.saved")) else order=defaults end
local function present(list)
    local out={}
    for _,id in ipairs(list) do out[id]=true end
    return out
end
local function glyph(id,name,color,size)
    local px=size or T.icons.large*4/3
    return ui.Text{id=id,text=name,family="Material Symbols Rounded",font_size=px,width=px+2,height=24,shrink=0,color=color or "onSurface"}
end
local function stack(id,items,spacing)
    spacing=spacing or GAP
    local kids={id=id,width=INNER,gap=spacing,shrink=0}
    local total=0
    for i,item in ipairs(items) do kids[#kids+1]=item[1] total=total+item[2]+(i>1 and spacing or 0) end
    kids.height=total
    return ui.Column(kids),total
end
local function header(title,back,tools)
    local row={id="header",width=INNER,height=HEAD,gap=GAP,align="center"}
    if back then row[#row+1]=kit.IconButton{id="back",text="arrow_back",aria_label=ui.tr("bead.panel.control-back"),width=HEAD,height=HEAD,on_activate=back} end
    row[#row+1]=ui.Text{id="title",text=title,height=HEAD,grow=1,shrink=1,font_size=T.text.headline*4/3,color="onSurface"}
    for _,tool in ipairs(tools or {}) do row[#row+1]=tool end
    return {ui.Row(row),HEAD}
end
local function message()
    local text=get("cc.message")
    if text=="" and get("cc.writable") then return nil end
    return {ui.Text{id="message",text=text~="" and text or ui.tr("bead.panel.control-readonly"),width=INNER,height=CAPTION*3,font_size=CAPTION,
        color=text~="" and "error" or "onSurfaceVariant"},CAPTION*3}
end
local function build_tile(t,w,h,embedded)
    local s=t.source and src[t.source] or {}
    local label=ui.tr(t.label)
    local function act(verb) return ui.action("cc.do",verb..":"..t.source) end
    local opener
    if t.dock and not embedded then opener=ui.action("detail",t.dock)
    elseif t.view and views[t.view] then opener=ui.action("view.open",t.view)
    elseif t.page then opener=ui.action("detail",t.id) end
    local id="tile-"..t.id
    if t.kind=="slider" then
        return kit.SliderTile{id=id,width=w,height=h,on_detail=opener,detail_label=label,icon=s.icon or t.icon,label=label,detail=s.status,value=s.value or 0,
            on_toggle=s.toggle and act("toggle") or nil,on_change=s.set and "cc.do" or nil,
            change_arg=function(n) return "set:"..t.source..":"..n end}
    elseif t.kind=="segmented" then
        -- Light | Dark | Auto as one control (QML NacreControlAppearance); the daemon names the mode that is set.
        local modes={{"light","light_mode","bead.panel.control-mode-light"},{"dark","dark_mode","bead.panel.control-mode-dark"},
            {"auto","brightness_auto","bead.panel.control-mode-auto"}}
        local seg=(w-2*T.spacing.xs)/#modes
        local row={id=id,width=w,height=h,gap=T.spacing.xs}
        for i,m in ipairs(modes) do
            local picked=s.mode==m[1]
            local ink=picked and "onPrimaryContainer" or "onSurface"
            local name=ui.tr(m[3])
            row[i]=kit.Button{id="mode-"..m[1],width=seg,height=h,padding=0,radius=T.radii.full,selected=picked,
                fill=picked and "primaryContainer" or "surfaceContainer",aria_label=ui.tr(m[3]),
                on_activate=s.toggle and ui.action("cc.do","choose:"..t.source..":"..m[1]) or nil,
                ui.Row{id="mode-row-"..m[1],width=seg,height=h,gap=T.spacing.xs,align="center",justify="center",
                    glyph("mode-icon-"..m[1],m[2],ink,T.icons.medium*4/3),
                    ui.Text{id="mode-label-"..m[1],text=name,width=(utf8.len(name) or 4)*8+4,height=22,shrink=0,font_size=CAPTION,color=ink}}}
        end
        return ui.Row(row)
    elseif t.kind=="link" then
        -- Link tiles open their dock page or their local choice list.
        local choosing=not t.dock and s.choices and #s.choices>0
        return kit.LinkTile{id=id,width=w,height=h,icon=s.icon or t.icon,label=label,detail=s.status,
            on_activate=choosing and ui.action("detail",t.id) or opener}
    elseif t.kind=="button" then
        return kit.ButtonTile{id=id,width=w,height=h,icon=t.icon,label=label,on_activate=s.press and act("press") or opener}
    end
    return kit.ToggleTile{id=id,width=w,height=h,icon=s.icon or t.icon,label=label,detail=s.status,on=s.on==true,
        on_activate=s.toggle and act("toggle") or opener,on_detail=s.toggle and opener or nil,
        detail_label=ui.tr("bead.panel.control-details",{name=label})}
end
local NOTICE_H=70
local function notice_card(n)
    return kit.Card{id="notice-"..n.id,width=INNER,height=NOTICE_H,shrink=0,padding=T.spacing.sm,gap=0,padding_left=T.spacing.md,
        ui.Text{id="app",text=n.app,width=INNER-2*T.spacing.md,height=16,font_size=CAPTION,color="onSurfaceVariant"},
        ui.Text{id="summary",text=n.summary,width=INNER-2*T.spacing.md,height=22,font_size=BODY,color="onSurface"},
        ui.Text{id="body",text=n.body,width=INNER-2*T.spacing.md,height=16,font_size=CAPTION,color="onSurfaceVariant"}}
end
local function notices_empty()
    local text=get("cc.notice_ready") and ui.tr("bead.panel.control-notices-empty") or ui.tr("bead.panel.control-notices-loading")
    return {ui.Text{id="notices-empty",text=text,width=INNER,height=CAPTION*2,font_size=CAPTION,color="onSurfaceVariant"},CAPTION*2}
end
-- The notification summary under the tiles (the QML NacreControlNotices): title, count, clear and "view all",
-- then the newest two entries; `limit` 0 on the full page, which lists every projected entry instead.
local function notices(limit,page)
    local list=get("cc.notices")
    local count=get("cc.notice_count")
    local clear=get("cc.notice_clear")
    local row={id="notices-head",width=INNER,height=HEAD,gap=GAP,align="center",
        ui.Text{id="notices-title",text=ui.tr("bead.panel.control-notifications"),height=HEAD,grow=1,shrink=1,font_size=T.text.title*4/3,color="onSurface"}}
    if count>0 then row[#row+1]=ui.Text{id="notices-count",text=tostring(count),height=HEAD,font_size=BODY,color="onSurfaceVariant"} end
    row[#row+1]=kit.IconButton{id="notices-clear",text="delete_sweep",aria_label=ui.tr("bead.panel.control-notices-clear"),width=HEAD,height=HEAD,
        on_activate=clear and ui.action("cc.do","clear:notifications") or nil}
    if not page then
        row[#row+1]=kit.IconButton{id="notices-all",text="chevron_right",aria_label=ui.tr("bead.panel.control-notices-all"),width=HEAD,height=HEAD,
            on_activate=ui.action("detail","notifications")}
    end
    local items={{ui.Row(row),HEAD}}
    if #list==0 then table.insert(items,notices_empty()) end
    for i,n in ipairs(list) do
        if not limit or i<=limit then table.insert(items,{notice_card(n),NOTICE_H}) end
    end
    return items
end
local function home()
    local status_w=INNER-3*HEAD-3*GAP
    local estimate=get("cc.battery_estimate")
    local status={id="status",width=INNER,height=HEAD,gap=GAP,align="center",
        kit.Button{id="battery-status",width=status_w,height=HEAD,padding=0,fill="transparent",hover_fill="transparent",
            aria_label=ui.tr("bead.panel.control-battery-details"),on_activate=views.battery and ui.action("detail","battery") or nil,
            ui.Row{id="battery-row",width=status_w,height=HEAD,gap=T.spacing.xs,align="center",
                glyph("battery-glyph",get("cc.battery_icon"),"primary"),
                ui.Text{id="battery-text",text=get("cc.battery"),width=estimate=="" and status_w-40 or BATTERY_W,height=24,font_size=BODY,color="onSurface"},
                estimate~="" and ui.Text{id="battery-estimate",text=estimate,width=status_w-40-BATTERY_W-T.spacing.xs,height=24,font_size=CAPTION,color="onSurfaceVariant"} or nil}},
        kit.IconButton{id="edit",text="edit",aria_label=ui.tr("bead.panel.control-edit"),width=HEAD,height=HEAD,
            on_activate=ui.action("begin",table.concat(order,","))},
        kit.IconButton{id="settings",text="settings",aria_label=ui.tr("bead.panel.control-settings"),width=HEAD,height=HEAD,
            on_activate=ui.action("cc.do","open:settings")},
        kit.IconButton{id="power",text="power_settings_new",aria_label=ui.tr("bead.panel.control-power"),width=HEAD,height=HEAD,
            on_activate=views.session and ui.action("view.open","session") or nil}}
    local list={}
    for _,id in ipairs(order) do
        local t=reg.by_id[id]
        if t and supported(t) then
            local ok=usable(t)
            -- Supported but temporarily unavailable tiles retain their explanatory state.
            table.insert(list,{size=t.size,build=function(w,h)
                local tile=build_tile(t,w,h)
                if ok then return tile end
                return ui.Box{id="dim-"..t.id,width=w,height=h,tile,
                    ui.Box{id="veil",position="absolute",left=0,top=0,width=w,height=h,radius=T.radii.medium,fill=ui.alpha("surface",0.6)}}
            end})
        end
    end
    local items={{ui.Row(status),HEAD}}
    if #list==0 then
        table.insert(items,{ui.Text{id="empty",text=ui.tr("bead.panel.control-empty"),width=INNER,height=CAPTION*3,font_size=CAPTION,color="onSurfaceVariant"},CAPTION*3})
    else
        local grid,height=kit.TileGrid{id="grid",width=INNER,gap=GAP,sizes=reg.layout.sizes,tiles=list}
        table.insert(items,{grid,height})
    end
    for _,item in ipairs(notices(2,false)) do table.insert(items,item) end
    return stack("home",items)
end
local function row_button(id,icon,label,detail,action,sub)
    local ink=sub and "onSurfaceVariant" or "onSurface"
    local text_w=INNER-ROW-T.spacing.lg*2
    local lines={ui.Text{id=id.."-label",text=label,width=text_w,height=sub and 20 or 22,font_size=sub and CAPTION or BODY,color=ink}}
    if sub then lines[2]=ui.Text{id=id.."-sub",text=sub,width=text_w,height=18,font_size=CAPTION,color="onSurfaceVariant"} end
    return kit.Button{id=id,width=INNER,height=ROW,padding=0,radius=T.radii.medium,on_activate=action,aria_label=label,
        ui.Row{id=id.."-row",width=INNER,height=ROW,padding_left=T.spacing.md,gap=T.spacing.md,align="center",
            glyph(id.."-icon",icon,ink),ui.Column(table.move(lines,1,#lines,1,{id=id.."-text",width=text_w})),
            glyph(id.."-detail",detail,ink)}}
end
local function edit()
    local visible={}
    for _,id in ipairs(order) do
        local t=reg.by_id[id]
        if t and supported(t) then table.insert(visible,{id=id,label=ui.tr(t.label),icon=t.icon}) end
    end
    for _,item in ipairs(visible) do
        item.build=function(w,h)
            return ui.Row{id="item",width=w,height=h,padding_left=T.spacing.md,gap=T.spacing.md,align="center",
                glyph("handle","drag_indicator","onSurfaceVariant"),glyph("icon",item.icon,"primary"),
                ui.Text{id="label",text=item.label,width=w-h-T.spacing.md*2-2*(T.icons.large*4/3+2)-T.spacing.md*2,height=22,font_size=BODY,color="onSurface"}}
        end
    end
    local items={header(ui.tr("bead.panel.control-edit-title"),ui.action("page","home"),{
        kit.Button{id="reset",width=HEAD*2,height=HEAD,padding=0,radius=T.radii.full,fill="surfaceContainer",
            aria_label=ui.tr("bead.panel.control-reset"),on_activate=ui.action("reset",table.concat(defaults,",")),
            ui.Row{id="reset-row",width=HEAD*2,height=HEAD,align="center",justify="center",
                ui.Text{id="reset-label",text=ui.tr("bead.panel.control-reset"),width=HEAD*2-T.spacing.md,height=22,text_align="center",font_size=BODY,color="onSurface"}}}}),
        {ui.Text{id="hint",text=ui.tr("bead.panel.control-edit-hint"),width=INNER,height=CAPTION*3,font_size=CAPTION,color="onSurfaceVariant"},CAPTION*3}}
    if #visible>0 then
        table.insert(items,{kit.ReorderList{id="order",width=INNER,item_height=ROW,gap=GAP,items=visible,on_touch="touch",
            remove=function(item,h)
                return kit.IconButton{id="remove",text="close",aria_label=ui.tr("bead.panel.control-remove",{name=item.label}),width=h,height=h,
                    on_activate=ui.action("remove",item.id)}
            end},#visible*(ROW+GAP)-GAP})
    end
    table.insert(items,{row_button("add","add",ui.tr("bead.panel.control-add"),"chevron_right",ui.action("page","catalog")),ROW})
    local note=message()
    if note then table.insert(items,note) end
    return stack("edit",items)
end
local function catalog()
    local inside=present(order)
    local ready,waiting={},{}
    for _,t in ipairs(reg.tiles) do
        if not inside[t.id] then table.insert(usable(t) and ready or waiting,t) end
    end
    local items={header(ui.tr("bead.panel.control-add"),ui.action("page","edit"))}
    for _,t in ipairs(ready) do
        local label=ui.tr(t.label)
        table.insert(items,{row_button("add-"..t.id,t.icon,label,"add",ui.action("add",t.id)),ROW})
    end
    if #waiting>0 then
        table.insert(items,{ui.Text{id="waiting",text=ui.tr("bead.panel.control-waiting"),width=INNER,height=24,font_size=BODY,color="onSurfaceVariant"},24})
        for _,t in ipairs(waiting) do
            table.insert(items,{row_button("wait-"..t.id,t.icon,ui.tr(t.label),"hourglass_empty",nil,ui.tr("bead.panel.control-needs",{contract=t.ask or t.id})),ROW})
        end
    end
    return stack("catalog",items)
end
local function notifications_page()
    local dnd=src["notifications.dnd"]
    local items={header(ui.tr("bead.panel.control-notifications"),ui.action("page","home"))}
    if dnd and dnd.toggle then
        table.insert(items,{kit.Button{id="dnd-switch",width=INNER,height=HEAD+T.spacing.sm,padding=0,radius=T.radii.full,
            fill=dnd.on and "primary" or "surfaceContainerHigh",aria_label=ui.tr("bead.panel.control-dnd"),on_activate=ui.action("cc.do","toggle:notifications.dnd"),
            ui.Row{id="dnd-row",width=INNER,height=HEAD+T.spacing.sm,align="center",justify="center",
                ui.Text{id="dnd-label",text=ui.tr("bead.panel.control-dnd").." · "..(dnd.status or ""),width=INNER-T.spacing.lg*2,height=22,
                    text_align="center",font_size=BODY,color=dnd.on and "onPrimary" or "onSurface"}}},HEAD+T.spacing.sm})
    end
    for _,item in ipairs(notices(nil,true)) do table.insert(items,item) end
    return stack("detail",items)
end
local function detail()
    local id=get("cc.detail")
    local dock={network={function() return require("../status/network-body")({width=INNER}) end,"bead.panel.control-network"},
        bluetooth={function() return require("../status/bluetooth-body")({width=INNER}) end,"bead.panel.control-bluetooth"},
        battery={function() return require("../status/battery-body")({width=INNER}) end,"bead.panel.control-power-brightness"},
        audio={function() return require("../sound/body")({width=INNER}) end,"bead.panel.control-sound"}}
    if dock[id] then
        local node,height=dock[id][1]()
        local items={header(ui.tr(dock[id][2]),ui.action("page","home")),{node,height}}
        if id=="battery" then
            for _,tile in ipairs({reg.by_id.brightness,reg.by_id.keyboard}) do
                if supported(tile) then
                    table.insert(items,{build_tile(tile,INNER,reg.layout.sizes.full.height,true),reg.layout.sizes.full.height})
                end
            end
        end
        return stack("detail",items,T.spacing.lg)
    end
    if id=="notifications" or id=="dnd" then return notifications_page() end
    local t=reg.by_id[id]
    local s=t and t.source and src[t.source] or {}
    local items={header(t and ui.tr(t.label) or "",ui.action("page","home"))}
    if t and t.page then
        table.insert(items,{ui.Text{id="description",text=ui.tr(t.page),width=INNER,height=BODY*4.5,font_size=BODY,color="onSurfaceVariant"},BODY*4.5})
    end
    if t and s.toggle then
        table.insert(items,{kit.Button{id="switch",width=INNER,height=HEAD+T.spacing.sm,padding=0,radius=T.radii.full,fill=s.on and "surfaceContainerHigh" or "primary",
            aria_label=s.on and ui.tr("bead.panel.control-turn-off") or ui.tr("bead.panel.control-turn-on"),on_activate=ui.action("cc.do","toggle:"..t.source),
            ui.Row{id="switch-row",width=INNER,height=HEAD+T.spacing.sm,align="center",justify="center",
                ui.Text{id="switch-label",text=s.on and ui.tr("bead.panel.control-turn-off") or ui.tr("bead.panel.control-turn-on"),
                    width=INNER-T.spacing.lg*2,height=22,text_align="center",font_size=BODY,color=s.on and "onSurface" or "onPrimary"}}},HEAD+T.spacing.sm})
    end
    for _,c in ipairs(s.choices or {}) do
        table.insert(items,{row_button("choice-"..c.id,t.icon,c.name,c.active and "radio_button_checked" or "radio_button_unchecked",
            ui.action("cc.do","choose:"..t.source..":"..c.id)),ROW})
    end
    if t and s.choices and views[t.view or ""] then
        table.insert(items,{row_button("details","battery_full",ui.tr("bead.panel.control-battery-details"),"chevron_right",ui.action("view.open",t.view)),ROW})
    end
    local note=message()
    if note then table.insert(items,note) end
    return stack("detail",items)
end
local body,content
if page=="edit" then body,content=edit()
elseif page=="catalog" then body,content=catalog()
elseif page=="detail" then body,content=detail()
else body,content=home() end
local max=get("cc.max_height")
-- B0067: the dock is as tall as the framed viewport (the QML sidebar keeps the full frame height); long pages scroll.
local height=max
local inner=body
if content+2*PAD>max then
    inner=ui.Scroll{id="scroll",scroll=ui.bind("cc.scroll"),width=INNER,height=height-2*PAD,body}
end
return kit.Panel{id="control-center",edge=get("view.edge"),open=ui.bind("panel.open"),duration=0.28,width=W,height=height,padding=PAD,
    gap=GAP,radius=T.radii.medium,fill="surface",inner}
