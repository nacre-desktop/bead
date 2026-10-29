-- SPDX-License-Identifier: GPL-3.0-only
-- Threshold transfer payloads and passive morph preview shared by every widget host.
local context=require("context")
local ui,kit=context.ui,context.kit
local registry=require("registry")
local preview=require("app/preview")
local t=context.t
local M={}

local function document_value(value)
    if type(value)~="table" then return nil end
    if type(value[1])=="table" and type(value[1].hosts)=="table" then return value[1] end
    if type(value.hosts)=="table" then return value end
end
local function group_state(document,item)
    local group=item.group or item.iid
    for name,entry in document.widgets or {} do
        if entry.widget==item.widget and (name==group or not group) then return name,entry end
    end
    return group,document.widgets and document.widgets[group] or nil
end
local function settings_for(package,state,item)
    local values=table.clone(package.defaults)
    for key,value in state and state.settings or {} do values[key]=value end
    for key,value in item.settings or {} do values[key]=value end
    return values
end
-- Opaque future settings stay in the canonical document, not in pointer metadata.
local function transfer_settings(package,values)
    local result={}
    for key in package.settings do if values[key]~=nil then result[key]=values[key] end end
    return result
end
local function form_for(document,group,state,item,form,package)
    local config=state and state.forms and state.forms[form] or {}
    local current=(item.host=="canvas" and "desktop" or "bar")==form
    local style=config.style
    if (style==nil or style=="") and current then style=item.style end
    if style==nil or style=="" then
        local alias=package.forms[form].legacy_style_setting
        style=alias and settings_for(package,state,item)[alias] or package.forms[form].default_style
    end
    return config,style
end

local function frozen_source(iid)
    if ui.resolve_for_check(ui.bind("widgets.drag.active"))~=true then return nil end
    local value=ui.resolve_for_check(ui.bind("widgets.drag.source"))
    local source=type(value)=="table" and (value[1] or value) or nil
    if type(source)=="table" and source.iid==iid then return source end
end

function M.payload(document,item)
    local source=frozen_source(item.iid)
    if source then return source end
    local package=registry.catalogue[item.widget]
    if not package then return nil end
    local host=item.host=="canvas" and "canvas" or "top_bar"
    local form=host=="canvas" and "desktop" or "bar"
    local group,state=group_state(document,item)
    local _,style=form_for(document,group,state,item,form,package)
    return {iid=item.iid,group=group,widget=item.widget,host=host,form=form,style=style,
        settings=transfer_settings(package,settings_for(package,state,item))}
end

local function group_drag_target(node)
    local marked,interactive={},{}
    local function scan(current)
        if type(current)~="table" then return end
        local props=current.props
        if type(props)=="table" then
            if props.drag_payload~=nil then table.insert(marked,props) end
            if props.on_activate~=nil then table.insert(interactive,props) end
        end
        for _,child in current.children or {} do scan(child) end
    end
    scan(node)
    if #marked==1 then return marked[1] end
    if #marked>1 then return nil end
    if #interactive==1 then return interactive[1] end
end

local prepared_renders=setmetatable({}, {__mode="k"})
local function prepare_node(node,alpha,spacer)
    if type(node)~="table" then return end
    local props=node.props
    if type(props)=="table" then
        local kind=type(node.kind)=="string" and string.lower(node.kind :: string) or ""
        if spacer then
            props.on_activate=nil
            props.on_drag=nil
            props.on_drop=nil
            props.drag_payload=nil
            props.activate_on_release=nil
            if kind=="button" then props.focus_on_pointer=false end
        elseif kind=="button" and props.on_activate~=nil then
            -- Preserve the original route, but activate it on release so a
            -- threshold drag cannot also open the widget's click action.
            props.activate_on_release=true
            props.focus_on_pointer=false
        end
        if alpha~=nil then
            for _,key in {"fill","hover_fill","selected_fill","color","border_color"} do
                local value=props[key]
                if (type(value)=="string" and value~="transparent")
                    or (type(value)=="table" and (value.role~=nil or value.base~=nil)) then
                    props[key]=ui.alpha(value,alpha)
                end
            end
        end
        if type(props.render)=="function" then
            local render=props.render
            local variants=prepared_renders[render]
            if not variants then variants=setmetatable({}, {__mode="v"}); prepared_renders[render]=variants end
            local mode=spacer and "spacer" or alpha==nil and "normal" or "alpha:"..tostring(alpha)
            local wrapper=variants[mode]
            if not wrapper then
                wrapper=function(...)
                    local generated=render(...)
                    prepare_node(generated,alpha,spacer)
                    return generated
                end
                variants[mode]=wrapper
            end
            props.render=wrapper
        end
    end
    for _,child in node.children or {} do prepare_node(child,alpha,spacer) end
end

local function hovered(item)
    local id=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
    local key="widget-"..tostring(item.iid)
    return string.find(id,key,1,true)~=nil or string.find(id,tostring(item.iid),1,true)~=nil
end

local function resize_handle(document,item)
    local frozen=frozen_source(item.iid)
    if item.host~="canvas" or ui.resolve_for_check(ui.bind("widgets.app.open"))~=true or not (hovered(item) or frozen and frozen.kind=="resize") then return nil end
    local package=registry.catalogue[item.widget]
    local form=package and package.forms.desktop
    if not form then return nil end
    local group,state=group_state(document,item)
    local config,style_name=form_for(document,group,state,item,"desktop",package)
    local style=form.styles[style_name]
    if not style then return nil end
    local slot=item.slot or {}
    local canvas=document.hosts.canvas or {}
    local source=frozen and frozen.kind=="resize" and frozen or {kind="resize",iid=item.iid,group=group,widget=item.widget,host="canvas",form="desktop",
        style=style_name,settings=settings_for(package,state,item),slot=table.clone(slot),
        min_width=style.min_width or style.min_span,max_width=style.max_width or style.max_span,
        preferred_width=style.preferred_width or style.preferred_span,
        min_height=style.min_height or kit.tokens.spacing.xl*2,
        max_height=style.max_height or tonumber(canvas.height) or tonumber(slot.h) or 1,
        preferred_height=style.preferred_height}
    return ui.Button{id="resize-"..tostring(item.iid),position="absolute",right=0,bottom=0,
        width=kit.tokens.spacing.xl,height=kit.tokens.spacing.xl,padding=0,radius=kit.tokens.radii.small,
        fill="surfaceContainerHighest",hover_fill=ui.alpha("primary",.24),focus_on_pointer=false,
        aria_label="Resize "..tostring(package.title),drag_payload={source},on_drag=ui.action("widgets.drag"),
        ui.Text{id="resize-glyph",text="south_east",family="Material Symbols Rounded",
            font_size=kit.tokens.icons.small,width=kit.tokens.spacing.xl,height=kit.tokens.spacing.xl,
            text_align="center",color="primary"}}
end

function M.attach(document,item,node)
    if not node or type(node.props)~="table" then return node end
    local source=frozen_source(item.iid)
    if item.drag_placeholder==true then
        prepare_node(node,0,true)
        return node
    end
    local payload=M.payload(document,item)
    if payload then
        if node.kind=="group" then
            local target=group_drag_target(node)
            if target then
                target.drag_payload={payload}
                target.on_drag=ui.action("widgets.drag")
            elseif type(node.props.width)=="number" and node.props.width>0
                and type(node.props.height)=="number" and node.props.height>0 then
                local props={id="drag-source-"..tostring(item.iid),drag_payload={payload},on_drag=ui.action("widgets.drag"),
                    width=node.props.width,height=node.props.height,shrink=node.props.shrink}
                table.insert(props,node)
                node=ui.Box(props)
            end
        else
            node.props.drag_payload={payload}
            node.props.on_drag=ui.action("widgets.drag")
        end
    end
    local marker=node.kind=="group" and group_drag_target(node) or node.props
    if marker then marker.metadata={{widget_instance=true,iid=item.iid}} end
    prepare_node(node,source and source.kind~="resize" and .3 or nil,false)
    local handle=resize_handle(document,item)
    if handle then table.insert(node.children,handle) end
    return node
end

function M.app_payload(document,widget,form,style,settings,selected_group)
    local package=registry.catalogue[widget]
    if not package or not package.forms[form] then return nil end
    local group,state=group_state(document,{widget=widget,group=selected_group})
    local values=table.clone(package.defaults)
    for key,value in state and state.settings or {} do values[key]=value end
    for key,value in settings or {} do values[key]=value end
    if style==nil or style=="" then style=package.forms[form].default_style end
    return {group=group,widget=widget,host=form=="desktop" and "canvas" or "top_bar",form=form,
        style=style,settings=transfer_settings(package,values)}
end

function M.ghost_node()
    if ui.resolve_for_check(ui.bind("widgets.drag.active"))~=true then return nil end
    local source_value=ui.resolve_for_check(ui.bind("widgets.drag.source"))
    local source=type(source_value)=="table" and (source_value[1] or source_value) or nil
    if type(source)~="table" or type(source.widget)~="string" or source.kind=="resize" then return nil end
    local origin_valid=ui.resolve_for_check(ui.bind("widgets.drag.source_origin_valid"))==true
    local source_is_app=source.iid==nil
    local app_origin_valid=ui.resolve_for_check(ui.bind("widgets.drag.app_origin_valid"))==true
    if not origin_valid and not (source_is_app and app_origin_valid) then return nil end
    local phase=ui.resolve_for_check(ui.bind("widgets.drag.phase"))
    local active=ui.resolve_for_check(ui.bind("widgets.drag.active"))==true
    local document=(active and (phase=="dragging" or phase=="settle")
        and document_value(ui.resolve_for_check(ui.bind("widgets.drag.layout"))))
        or document_value(ui.resolve_for_check(ui.bind("widgets.draft")))
        or document_value(ui.resolve_for_check(ui.bind("widgets.layout")))
    if not document then return nil end
    local package=registry.catalogue[source.widget]
    if not package then return nil end
    local group=source.group
    local state=group and document.widgets and document.widgets[group]
    if not state then group,state=group_state(document,source) end
    local destination=ui.resolve_for_check(ui.bind("widgets.drag.destination"))
    local phase=ui.resolve_for_check(ui.bind("widgets.drag.phase")) or "dragging"
    if phase=="" then phase="dragging" end
    local form=phase=="return" and (source.form or (source.host=="canvas" and "desktop" or "bar"))
        or destination=="desktop" and "desktop"
        or destination=="bar" and "bar"
        or source.form or "bar"
    if destination=="desktop" and not package.forms.desktop then form="bar" end
    if form~="bar" and form~="desktop" or not package.forms[form] then return nil end
    local config,style_name=form_for(document,group,state,source,form,package)
    local style=package.forms[form].styles[style_name]
    if not style then return nil end
    local settings=settings_for(package,state,{settings=source.settings})
    local screen_width=math.max(1,ui.resolve_for_check(ui.bind("bead.viewport.width")) or 1)
    local bar=package.variants.bar
    local source_width=math.max(1,ui.resolve_for_check(ui.bind("widgets.drag.width")) or 1)
    local source_height=math.max(1,ui.resolve_for_check(ui.bind("widgets.drag.height")) or 1)
    local target_width,target_height
    if form=="bar" then
        local bar_layout=document.hosts.top_bar
        local span=tonumber(config.placement and config.placement.slot and config.placement.slot.span) or style.preferred_span or 1
        target_width=bar.reserve_width and bar.reserve_width(settings,style_name)
            or span*(screen_width/math.max(1,bar_layout and bar_layout.tiles or 1))
        target_height=ui.resolve_for_check(ui.bind("bar.height")) or t.status_height or 50
    else
        local placement=config.placement and config.placement.slot or {}
        target_width=tonumber(placement.w) or tonumber(style.preferred_span)
        target_height=tonumber(placement.h) or kit.tokens.spacing.huge*3
    end
    if source.iid~=nil and source.form==form then
        target_width,target_height=source_width,source_height
    end
    local pointer_x=ui.resolve_for_check(ui.bind("widgets.drag.x")) or 0
    local pointer_y=ui.resolve_for_check(ui.bind("widgets.drag.y")) or 0
    local grab_x=ui.resolve_for_check(ui.bind("widgets.drag.grab_x")) or source_width/2
    local grab_y=ui.resolve_for_check(ui.bind("widgets.drag.grab_y")) or source_height/2
    local preparing=phase=="prepare"
    local animated_width=ui.animate("widgets.drag.ghost.width",preparing and source_width or target_width,
        preparing and 0 or .18,"out-cubic")
    local animated_height=ui.animate("widgets.drag.ghost.height",preparing and source_height or target_height,
        preparing and 0 or .18,"out-cubic")
    -- Bake the real destination-sized content while the canvas is unmapped, but
    -- leave the visible animation at the bar-sized source for the first frame.
    local width=preparing and target_width or animated_width
    local height=preparing and target_height or animated_height
    local scale_x=animated_width/source_width
    local scale_y=animated_height/source_height
    local pointer_left,pointer_top=pointer_x-grab_x*scale_x,pointer_y-grab_y*scale_y
    local left_target,top_target=pointer_left,pointer_top
    if phase=="dragging" and destination=="desktop" then
        for _,item in document.hosts.canvas and document.hosts.canvas.instances or {} do
            if item.widget==source.widget and (item.group or item.iid)==group
                and (item.drag_placeholder==true or item.iid==source.iid) then
                local slot=item.slot
                if math.abs(slot.x-pointer_left)<=kit.tokens.spacing.lg then left_target=slot.x end
                if math.abs(slot.y-pointer_top)<=kit.tokens.spacing.lg then top_target=slot.y end
                break
            end
        end
    end
    if phase=="settle" or phase=="return" then
        local snap_x=ui.resolve_for_check(ui.bind("widgets.drag.snap_x")) or pointer_left
        local snap_y=ui.resolve_for_check(ui.bind("widgets.drag.snap_y")) or pointer_top
        left_target,top_target=snap_x,snap_y
    end
    local settling=phase=="settle" or phase=="return"
    local position_duration=preparing and 0 or settling and .22 or .06
    local position_curve=settling and "out-back" or "out-cubic"
    local left=ui.animate("widgets.drag.ghost.left",left_target,position_duration,position_curve)
    local top=ui.animate("widgets.drag.ghost.top",top_target,position_duration,position_curve)
    local node=preview.render(source.widget,form,style_name,settings,width,height,true)
    if not node then return nil end
    local marker=form=="bar" and ui.Box{id="widget-snap-marker",position="absolute",right=0,top=0,
        width=kit.tokens.spacing.xs,height=height,radius=kit.tokens.radii.small,
        fill=ui.alpha("primary",.9)} or nil
    return ui.Box{id="widgets-drag-ghost",position="absolute",
        left=left,top=top,width=width,height=height,clip_children=true,
        radius=kit.tokens.radii.medium,fill=ui.alpha("surfaceContainerHighest",.96),
        border_width=ui.animate("widgets.drag.ghost.border",phase=="settle" and 2.5 or 1.5,
            preparing and 0 or .14,"out-cubic"),border_color="primary",
        node,marker}
end

-- B0198 edit mode: while a widget is dragged, the desktop shows where it
-- can land and a removal target appears for placed widgets; the bar outlines itself
-- (bar/main.luau, drawn on the bar surface). Passive; the drop model
-- in the app behaviour decides with the same zone geometry (M.remove_zone).
-- The removal target, centred near the bottom of the output.
function M.remove_zone()
    local sp=kit.tokens.spacing
    return {width=sp.huge*6,height=sp.huge*2,bottom=sp.huge*2}
end
local function zone(id,left,top,width,height,label,icon,active,danger)
    local accent=danger and "error" or "primary"
    return ui.Box{id=id,position="absolute",left=left,top=top,width=width,height=height,
        radius=kit.tokens.radii.medium,fill=ui.alpha(accent,active and .16 or .05),
        border_width=active and 2 or 1.5,border_color=ui.alpha(accent,active and .9 or .45),
        align="center",justify="center",
        label and ui.Row{id=id.."-label",height=kit.tokens.spacing.huge,padding_left=kit.tokens.spacing.md,padding_right=kit.tokens.spacing.md,
            gap=kit.tokens.spacing.xs,align="center",radius=kit.tokens.radii.full,
            fill=ui.alpha(danger and "errorContainer" or "surfaceContainerHighest",active and .96 or .82),
            ui.Text{id="icon",text=icon,family="Material Symbols Rounded",font_size=kit.tokens.icons.large,
                width=kit.tokens.icons.large+kit.tokens.spacing.xs,height=kit.tokens.spacing.xl,color=danger and "onErrorContainer" or accent},
            ui.Text{id="text",text=label,font_size=kit.tokens.text.body*4/3,
                width=math.ceil(ui.text_width(label,"IBM Plex Sans",kit.tokens.text.body*4/3))+kit.tokens.spacing.hairline,
                height=kit.tokens.spacing.xl,color=danger and "onErrorContainer" or "onSurface"}} or nil}
end
function M.drop_zones(width,height)
    if ui.resolve_for_check(ui.bind("widgets.drag.active"))~=true
        or ui.resolve_for_check(ui.bind("widgets.drag.phase"))~="dragging" then return {} end
    local value=ui.resolve_for_check(ui.bind("widgets.drag.source"))
    local source=type(value)=="table" and (value[1] or value) or nil
    if type(source)~="table" or source.kind=="resize" or type(source.widget)~="string" then return {} end
    local package=registry.catalogue[source.widget]
    if not package then return {} end
    local destination=ui.resolve_for_check(ui.bind("widgets.drag.destination"))
    local bar_height=ui.resolve_for_check(ui.bind("bar.height")) or t.status_height or 50
    local inset=kit.tokens.spacing.md
    local zones={}
    local remove=M.remove_zone()
    local remove_left,remove_top=(width-remove.width)/2,height-remove.bottom-remove.height
    local x=ui.resolve_for_check(ui.bind("widgets.drag.x")) or -1
    local y=ui.resolve_for_check(ui.bind("widgets.drag.y")) or -1
    local over_remove=source.iid~=nil and x>=remove_left and x<remove_left+remove.width
        and y>=remove_top and y<remove_top+remove.height
    if package.forms.desktop then
        table.insert(zones,zone("drop-zone-desktop",inset,bar_height*1.5,width-2*inset,height-bar_height*1.5-inset,
            ui.tr("bead.widgets.zone-desktop"),"wallpaper",destination=="desktop" and not over_remove,false))
    end
    if source.iid~=nil then
        table.insert(zones,zone("drop-zone-remove",remove_left,remove_top,remove.width,remove.height,
            ui.tr("bead.widgets.zone-remove"),"delete",over_remove,true))
    end
    return zones
end
function M.ghost_root()
    local width=ui.resolve_for_check(ui.bind("bead.viewport.width")) or 1
    local height=ui.resolve_for_check(ui.bind("bead.viewport.height")) or 1
    local props={id="widgets-drag-overlay",width=width,height=height}
    for _,node in M.drop_zones(width,height) do table.insert(props,node) end
    local node=M.ghost_node()
    if node then table.insert(props,node) end
    return ui.Group(props)
end

return M
