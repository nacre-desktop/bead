-- SPDX-License-Identifier: GPL-3.0-only
-- Render real widget package forms as passive, scaled previews.
local context=require("../context")
local ui,kit=context.ui,context.kit
local registry=require("../registry")
local M={}

local scaled={width=true,height=true,min_width=true,max_width=true,left=true,right=true,top=true,bottom=true,
    padding=true,padding_left=true,padding_right=true,padding_top=true,padding_bottom=true,gap=true,gap_x=true,gap_y=true,
    radius=true,border_width=true,font_size=true,icon_size=true}

local function prepare(node,factor)
    if type(node)~="table" then return end
    local props=node.props or {}
    for key in props do
        if type(key)=="string" and (key:sub(1,3)=="on_" or key:sub(1,5)=="focus" or key:sub(1,7)=="pointer"
            or key:sub(1,4)=="drag" or key=="field" or key=="action" or key=="drag_payload"
            or key=="activate_on_release" or key=="keyboard" or key=="aria_label") then
            props[key]=nil
        elseif scaled[key] then
            local value=ui.resolve_for_check(props[key])
            if type(value)=="number" then props[key]=value*factor end
        elseif key=="render" and type(props.render)=="function" then
            local render=props.render
            props.render=function(...)
                local generated=render(...)
                prepare(generated,factor)
                return generated
            end
        end
    end
    local kind=type(node.kind)=="string" and string.lower(node.kind) or ""
    if kind=="button" then
        node.kind="box"
    elseif kind=="textinput" or kind=="text_input" or kind=="input" then
        node.kind="text"
    end
    for _,child in node.children or {} do prepare(child,factor) end
    return node
end

local function render_item(package,instance,placement,settings,factor)
    local variant=package and package.variants[placement=="desktop" and "canvas" or "bar"]
    if not variant then return nil end
    local render
    render=function(item,child_variant)
        local child_package=registry.catalogue[item.widget]
        local child_values=table.clone(child_package.defaults)
        for key,value in item.settings or {} do child_values[key]=value end
        local child_props={instance=item,catalogue=registry.catalogue,render=render,editing=false,local_layout=false}
        return child_variant.render(child_props,child_values,{})
    end
    local node=variant.render({instance=instance,catalogue=registry.catalogue,render=render,editing=false,local_layout=false},settings,{})
    prepare(node,factor or 1)
    return node
end

function M.render_instance(instance,placement,factor)
    local package=instance and registry.catalogue[instance.widget]
    if not package then return nil end
    local values=table.clone(package.defaults)
    for key,value in instance.settings or {} do values[key]=value end
    return render_item(package,instance,placement,values,factor)
end

function M.render_bar(document,width,height,factor,selected,style,settings)
    local layout=document.hosts.top_bar
    if not layout then return nil end
    local function copy_item(item)
        local copy=table.clone(item)
        if item.widget==selected then
            copy.style=style
            copy.settings=settings
        end
        if item.children then
            copy.children={}
            for _,child in item.children do table.insert(copy.children,copy_item(child)) end
        end
        return copy
    end
    local instances={}
    for _,item in layout.instances or {} do
        table.insert(instances,copy_item(item))
    end
    local render
    render=function(item,variant)
        local package=registry.catalogue[item.widget]
        if not package then return nil end
        local values=table.clone(package.defaults)
        for key,value in item.settings or {} do values[key]=value end
        return variant.render({instance=item,catalogue=registry.catalogue,render=render,editing=false,
            local_layout=layout.mode=="tiles"},values,{})
    end
    local node=ui.Host{id="stage-top-bar",shape="strip-h",width=width,height=height,
        layout=instances,catalogue=registry.catalogue,render=render,
        grid={tiles=layout.tiles,mode=layout.mode,regions=layout.regions}}
    return prepare(node,factor or 1)
end

function M.render(id,placement,style,settings,width,height,actual_size)
    local package=registry.catalogue[id]
    if not package then return nil end
    local values=table.clone(package.defaults)
    for key,value in settings or {} do values[key]=value end
    local variant=package.variants[placement=="desktop" and "canvas" or "bar"]
    if not variant then return nil end
    local host=placement=="desktop" and "canvas" or "top_bar"
    local logical_w,logical_h=width,height
    local scale
    if placement=="desktop" and not actual_size then
        local form=package.forms.desktop
        local definition=form.styles[style or form.default_style]
        logical_w=definition.preferred_span
        logical_h=definition.preferred_height or kit.tokens.spacing.huge*3
        scale=math.min(1,width/logical_w,height/logical_h)
    elseif placement=="desktop" then
        scale=1
    else
        local natural=variant.reserve_width and variant.reserve_width(values) or variant.preferred_span*32
        scale=math.min(1,height/28,width/math.max(1,natural))
    end
    local instance={iid="preview-"..id,widget=id,host=host,style=style,settings=values,
        slot=placement=="desktop" and {x=0,y=0,w=logical_w,h=logical_h} or {region="left",start=0,span=variant.preferred_span},children={}}
    local node=render_item(package,instance,placement,values,scale)
    return ui.Box{id="preview-"..id,width=width,height=height,align="center",justify="center",clip_children=true,node}
end

return M
