-- SPDX-License-Identifier: GPL-3.0-only
-- Threshold transfer payloads and passive morph preview shared by every widget host.
local context=require("context")
local ui,kit=context.ui,context.kit
local registry=require("registry")
local preview=require("app/preview")
local t=context.t
local M={}
local generic=require("bead.widgets.drag")()

local function frozen_source(iid)
    if ui.resolve_for_check(ui.bind("widgets.drag.active"))~=true then return nil end
    local value=ui.resolve_for_check(ui.bind("widgets.drag.source"))
    local source=type(value)=="table" and (value[1] or value) or nil
    if type(source)=="table" and source.iid==iid then return source end
end

local function drag_options()
    return {catalogue=registry.catalogue,frozen_source=frozen_source,
        host_name=function(item) return item.host=="canvas" and "canvas" or "top_bar" end,
        form_name=function(item) return item.host=="canvas" and "desktop" or "bar" end,
        host_names={canvas="canvas"}}
end
function M.payload(document,item)
    return generic.payload(document,item,drag_options())
end
local function group_state(document,item) return generic.group_state(document,item) end
local function settings_for(package,state,item) return generic.settings_for(package,state,item) end
local function form_for(document,group,state,item,form,package)
    return generic.form_for(document,group,state,item,form,package,{canvas="canvas"})
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
    local source=frozen_source(item.iid)
    local payload=M.payload(document,item)
    return generic.attach_tree(item,node,payload,{
        drag_action=ui.action("widgets.drag"),
        wrapper_id=function(instance) return "drag-source-"..tostring(instance.iid) end,
        wrap=function(props) return ui.Box(props) end,
        metadata=function(instance) return {{widget_instance=true,iid=instance.iid}} end,
        alpha_for=function() return source and source.kind~="resize" and .3 or nil end,
        color_fields={"fill","hover_fill","selected_fill","color","border_color"},
        alpha=function(value,factor) return ui.alpha(value,factor) end,
        resize_handle=function() return resize_handle(document,item) end,
    })
end

function M.app_payload(document,widget,form,style,settings,selected_group)
    return generic.app_payload(document,widget,form,style,settings,selected_group,{
        catalogue=registry.catalogue,host_for_form=function(value) return value=="desktop" and "canvas" or "top_bar" end})
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
    -- While dragging the ghost draws from the frozen base: the transient bar preview changes
    -- with the pointer and is not part of the ghost (B0210).
    local document=(active and phase=="dragging" and generic.document_value(ui.resolve_for_check(ui.bind("widgets.drag.base"))))
        or (active and phase=="settle" and generic.document_value(ui.resolve_for_check(ui.bind("widgets.drag.layout"))))
        or generic.document_value(ui.resolve_for_check(ui.bind("widgets.draft")))
        or generic.document_value(ui.resolve_for_check(ui.bind("widgets.layout")))
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
    local geometry=generic.ghost_geometry({
        phase=phase,prepare_phase="prepare",settle_phase="settle",return_phase="return",
        source_width=source_width,source_height=source_height,target_width=target_width,target_height=target_height,
        pointer_x=pointer_x,pointer_y=pointer_y,grab_x=grab_x,grab_y=grab_y,
        snap_x=ui.resolve_for_check(ui.bind("widgets.drag.snap_x")),
        snap_y=ui.resolve_for_check(ui.bind("widgets.drag.snap_y")),
        animate=function(key,target,duration,easing) return ui.animate(key,target,duration,easing) end,
        width_key="widgets.drag.ghost.width",height_key="widgets.drag.ghost.height",
        x_key="widgets.drag.ghost.dx",y_key="widgets.drag.ghost.dy",
        size_duration=.18,size_easing="out-cubic",settle_duration=.22,settle_easing="out-back",
    })
    local width,height,left,top=geometry.width,geometry.height,geometry.left,geometry.top
    local children
    if source.iid~=nil then
        local function find(items)
            for _,item in items or {} do
                if item.iid==source.iid then return item.children end
                local nested=find(item.children)
                if nested then return nested end
            end
        end
        for _,host in document.hosts do children=children or find(host.instances) end
    end
    local node=preview.render(source.widget,form,style_name,settings,width,height,true,children)
    if not node then return nil end
    -- A bar widget has no card of its own, so its ghost gets a quiet one (B0211: no accent
    -- outline); a desktop widget is its own card.
    local carded=form=="bar"
    local ghost=ui.Box{id="widgets-drag-ghost",position="absolute",
        left=left,top=top,width=width,height=height,clip_children=true,
        radius=carded and kit.tokens.radii.medium or 0,fill=carded and ui.alpha("surfaceContainerHighest",.96) or "transparent",
        border_width=carded and kit.tokens.spacing.hairline or 0,border_color="outlineVariant",
        node}
    return ghost
end
-- B0198 edit mode: while a placed widget is dragged a removal target appears near the bottom
-- of the output (B0211: the only drag zone; no outlines on the desktop or the bar). Passive;
-- the drop model in the app behaviour decides with the same geometry (M.remove_zone).
function M.remove_zone()
    local sp=kit.tokens.spacing
    return {width=sp.huge*6,height=sp.huge*2,bottom=sp.huge*2}
end
function M.drop_zones(width,height)
    if ui.resolve_for_check(ui.bind("widgets.drag.active"))~=true
        or ui.resolve_for_check(ui.bind("widgets.drag.phase"))~="dragging" then return {} end
    local value=ui.resolve_for_check(ui.bind("widgets.drag.source"))
    local source=type(value)=="table" and (value[1] or value) or nil
    if type(source)~="table" or source.kind=="resize" or source.iid==nil
        or not registry.catalogue[source.widget] then return {} end
    local sp=kit.tokens.spacing
    local zone=M.remove_zone()
    local active=ui.resolve_for_check(ui.bind("widgets.drag.over_remove"))==true
    local label=ui.tr("bead.widgets.zone-remove")
    return {ui.Box{id="drop-zone-remove",position="absolute",left=(width-zone.width)/2,top=height-zone.bottom-zone.height,
        width=zone.width,height=zone.height,radius=kit.tokens.radii.medium,fill=ui.alpha("error",active and .16 or .05),
        border_width=active and 2 or 1.5,border_color=ui.alpha("error",active and .9 or .45),align="center",justify="center",
        ui.Row{id="drop-zone-remove-label",height=sp.huge,padding_left=sp.md,padding_right=sp.md,
            gap=sp.xs,align="center",radius=kit.tokens.radii.full,fill=ui.alpha("errorContainer",active and .96 or .82),
            ui.Text{id="icon",text="delete",family="Material Symbols Rounded",font_size=kit.tokens.icons.large,
                width=kit.tokens.icons.large+sp.xs,height=sp.xl,color="onErrorContainer"},
            ui.Text{id="text",text=label,font_size=kit.tokens.text.body*4/3,
                width=math.ceil(ui.text_width(label,"IBM Plex Sans",kit.tokens.text.body*4/3))+sp.hairline,
                height=sp.xl,color="onErrorContainer"}}}}
end
-- The ghost and the zones are separate canvases: the ghost surface moves with the pointer,
-- the zones stay put (B0210).
function M.ghost_root()
    local width=ui.resolve_for_check(ui.bind("bead.viewport.width")) or 1
    local height=ui.resolve_for_check(ui.bind("bead.viewport.height")) or 1
    return ui.Group{id="widgets-drag-overlay",width=width,height=height,M.ghost_node()}
end
function M.zones_root()
    local width=ui.resolve_for_check(ui.bind("bead.viewport.width")) or 1
    local height=ui.resolve_for_check(ui.bind("bead.viewport.height")) or 1
    local props={id="widgets-drag-zones",width=width,height=height}
    for _,node in M.drop_zones(width,height) do table.insert(props,node) end
    return ui.Group(props)
end

return M
