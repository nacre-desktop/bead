-- SPDX-License-Identifier: GPL-3.0-only
-- Edit affordances and destination geometry belong to the shell's host composition.
local c=require("context")
local ui,kit=c.ui,c.kit
local sp=kit.tokens.spacing
local M={}
local native_catalogue
local function descriptors()
    if native_catalogue then return native_catalogue end
    native_catalogue={}
    for id,package in require("registry").catalogue do
        local forms={}
        for name,form in package.forms do
            local styles={}
            for style_name,style in form.styles do
                local descriptor={title=style.title,view=style.view,min_span=style.min_span,max_span=style.max_span,
                    preferred_span=style.preferred_span,preferred_height=style.preferred_height,
                    requires=style.requires and table.clone(style.requires) or nil}
                styles[style_name]=descriptor
            end
            local clean={default_style=form.default_style,styles=styles}
            if form.legacy_style_setting then clean.legacy_style_setting=form.legacy_style_setting end
            forms[name]=clean
        end
        table.insert(native_catalogue,{id=id,title=package.title,icon=package.icon,category=package.category,
            defaults=table.clone(package.defaults),settings=table.clone(package.settings),forms=forms})
    end
    return native_catalogue
end
function M.requires(document,placement,binding)
    local host=document.hosts[placement=="desktop" and "canvas" or "top_bar"]
    local function scan(items)
        for _,item in items or {} do
            local package=require("registry").catalogue[item.widget]
            local form=package and package.forms[placement]
            local style=form and form.styles[item.style or form.default_style]
            if style and style.requires and table.find(style.requires,binding) then return true end
            if scan(item.children) then return true end
        end
        return false
    end
    return host and scan(host.instances) or false
end
function M.has_style(document,widget,style_name)
    local host=document.hosts.canvas
    local function scan(items)
        for _,item in items or {} do
            if item.widget==widget and item.style==style_name then return true end
            if scan(item.children) then return true end
        end
        return false
    end
    return host and scan(host.instances) or false
end
function M.seed(document,width,height,placement)
    local reservations={}
    local geometry=ui.resolve_for_check(ui.bind("widgets.geometry"))
    local actual_widths={}
    if type(geometry)=="table" then
        for iid,row in geometry do
            if type(iid)=="string" then
                local actual=tonumber(type(row)=="table" and (row.w or row.width) or row)
                if actual and actual>0 then actual_widths[iid]=actual end
            end
            if type(row)=="table" and type(row.iid)=="string" then
                local actual=tonumber(row.w or row.width)
                if actual and actual>0 then actual_widths[row.iid]=actual end
            end
        end
    end
    local tile_width=width/math.max(1,document.hosts.top_bar.tiles or 1)
    local function reserve(items)
        local total_extra=0
        for _,item in items or {} do
            local children_extra=reserve(item.children)
            local package=require("registry").catalogue[item.widget]
            local variant=package and package.variants.bar
            local actual=actual_widths[item.iid]
            if not actual and item.slot and type(item.slot.span)=="number" then
                actual=item.slot.span*tile_width
            end
            actual=actual or 0
            local selected_reserve
            if variant and variant.reserve_width then
                local settings=table.clone(package.defaults)
                local state=document.widgets and document.widgets[item.group or item.iid]
                for key,value in state and state.settings or {} do settings[key]=value end
                for key,value in item.settings or {} do settings[key]=value end
                local form=package.forms.bar
                local selected=item.style
                if type(selected)~="string" or selected=="" then selected=form.legacy_style_setting and settings[form.legacy_style_setting] end
                if type(selected)~="string" or selected=="" then selected=form.default_style end
                selected_reserve=variant.reserve_width(settings,selected)
            end
            if item.widget=="pill" and item.children and #item.children>0 then
                local combined=actual+children_extra
                if combined>0 then table.insert(reservations,{iid=item.iid,width=combined}) end
                total_extra+=children_extra
            else
                if selected_reserve and selected_reserve>0 then
                    table.insert(reservations,{iid=item.iid,width=math.max(actual,selected_reserve)})
                    total_extra+=math.max(0,selected_reserve-actual)
                end
                total_extra+=children_extra
            end
        end
        return total_extra
    end
    reserve(document.hosts.top_bar.instances)
    return {{widget_layout=true,document=document,initial=require("default")(width),rev=ui.resolve_for_check(ui.bind("widgets.rev")),
        widget_clock_seconds=placement~=nil and M.requires(document,placement,"bead.desktop.clock.seconds") or false,
        widget_clock_analog=placement=="desktop" and M.has_style(document,"clock","analog") or false,
        geometry=geometry,reservations=reservations,canvas_width=width,canvas_height=height,
        catalogue=descriptors(),snap_distance=sp.lg}}
end
function M.request(_document,extra)
    return ui.action("widgets.edit",{extra})
end
local function marked_drag_target(root)
    local match,count
    local function visit(node)
        if type(node)~="table" then return end
        if type(node.props)=="table" and node.props.drag_payload~=nil then
            match=node
            count=(count or 0)+1
        end
        for _,child in node.children or {} do visit(child) end
    end
    visit(root)
    if count==1 then return match end
end
local function contains_widget(hover,iid)
    return type(iid)=="string" and iid~="" and string.find(hover,iid,1,true)~=nil
end
local function nested_widget_hovered(item,hover)
    for _,child in item.children or {} do
        if contains_widget(hover,child.iid) or nested_widget_hovered(child,hover) then return true end
    end
    return false
end
local function decorate_target(document,item,target,host,hovered)
    local p=target.props
    p.fill=hovered and ui.alpha("primary",.025) or p.fill
    p.selected=hovered or p.selected==true
    if hovered then
        p.border_width=sp.hairline*1.5
        p.border_color="primary"
        target.children=target.children or {}
        table.insert(target.children,kit.IconButton{id="remove",position="absolute",right=-sp.xs,top=0,
            width=sp.md,height=sp.md,padding=0,font_size=sp.md,text="close",focus_on_pointer=false,
            aria_label="Remove "..tostring(item.widget),
            on_activate=M.request(document,{op="toggle-form",widget=item.widget,group=item.group or item.iid,
                form=host,enabled=false})})
    end
end
function M.decorate(document,item,node)
    if not node then return nil end
    local p=node.props or {}
    local hover=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
    local key="widget-"..tostring(item.iid)
    local matched=string.find(hover,key,1,true)~=nil or string.find(hover,tostring(item.iid),1,true)~=nil
    local hovered=matched and not nested_widget_hovered(item,hover)
    local host=item.host=="canvas" and "desktop" or "bar"
    if node.kind=="group" then
        local target=marked_drag_target(node)
        if target then
            decorate_target(document,item,target,host,hovered)
            return node
        elseif type(p.width)~="number" or type(p.height)~="number" then
            return node
        end
    end
    local props={id=key,padding=0,fill=hovered and ui.alpha("primary",.025) or "transparent",
        selected=hovered,border_width=hovered and sp.hairline*1.5 or 0,border_color="primary",radius=kit.tokens.radii.small,node}
    if p.width~=nil then props.width=p.width end
    if p.height~=nil then props.height=p.height end
    if p.shrink~=nil then props.shrink=p.shrink end
    if hovered then
        table.insert(props,kit.IconButton{id="remove",position="absolute",right=-sp.xs,top=0,
            width=sp.md,height=sp.md,padding=0,font_size=sp.md,text="close",focus_on_pointer=false,
            aria_label="Remove "..tostring(item.widget),
            on_activate=M.request(document,{op="toggle-form",widget=item.widget,group=item.group or item.iid,
                form=host,enabled=false})})
    end
    return ui.Box(props)
end
return M
