-- SPDX-License-Identifier: MIT
-- Generic placement form contracts and variant rendering. The host supplies UI/plugin hooks.
local M={}
local function integer(n,lo,hi) return type(n)=="number" and n%1==0 and n>=lo and n<=hi end
local colour_fields={fill=true,color=true,border_color=true,hover_fill=true,selected_fill=true,
    focus_color=true,shadow_color=true,from=true,to=true}
local function map_role(role,from,to)
    if type(role)~="string" then return role end
    local from_cap=from:sub(1,1):upper()..from:sub(2)
    local to_cap=to~="neutral" and to:sub(1,1):upper()..to:sub(2) or ""
    if role==from then return to=="neutral" and "surfaceVariant" or to end
    if role=="on"..from_cap then return to=="neutral" and "onSurfaceVariant" or "on"..to_cap end
    if role==from_cap.."Container" then return to=="neutral" and "surfaceContainerHigh" or to_cap.."Container" end
    if role=="on"..from_cap.."Container" then return to=="neutral" and "onSurface" or "on"..to_cap.."Container" end
    return role
end
local function map_colour(value,from,to)
    if type(value)=="string" then return map_role(value,from,to) end
    if type(value)~="table" then return value end
    local copy=table.clone(value)
    for key,item in copy do
        if key=="role" or key=="base" or key=="over" or key=="from" or key=="to" then
            copy[key]=map_colour(item,from,to)
        elseif type(item)=="table" then
            copy[key]=map_colour(item,from,to)
        end
    end
    return copy
end
local function map_widget_colour(node,from,to)
    if type(node)~="table" then return node end
    local props=node.props
    if type(props)=="table" then
        for key in colour_fields do
            if props[key]~=nil then props[key]=map_colour(props[key],from,to) end
        end
        if type(props.render)=="function" then
            local render=props.render
            props.render=function(...) return map_widget_colour(render(...),from,to) end
        end
    end
    for _,child in node.children or {} do map_widget_colour(child,from,to) end
    return node
end
local function make_variant(package,placement,form,ui)
    assert(type(form)=="table" and type(form.styles)=="table" and type(form.default_style)=="string",
        "invalid "..placement.." forms: "..package.id)
    local default=form.styles[form.default_style]
    assert(type(default)=="table","missing default "..placement.." style: "..package.id)
    local minimum,maximum=math.huge,0
    local has_reservation=false
    for name,style in form.styles do
        assert(type(name)=="string" and type(style)=="table" and type(style.title)=="string"
            and type(style.render)=="function","invalid "..placement.." style: "..package.id.."/"..tostring(name))
        assert(integer(style.min_span,1,20000) and integer(style.max_span,style.min_span,20000)
            and integer(style.preferred_span,style.min_span,style.max_span),
            "invalid "..placement.." size contract: "..package.id.."/"..name)
        if style.reserve_width then assert(type(style.reserve_width)=="function","invalid style width reservation") end
        if style.requires then
            assert(type(style.requires)=="table","invalid style requirements")
            for _,requirement in style.requires do assert(type(requirement)=="string","style requirement must name a data binding") end
        end
        minimum=math.min(minimum,style.min_span)
        maximum=math.max(maximum,style.max_span)
        if style.reserve_width then has_reservation=true end
    end
    assert(default.min_span and default.max_span and default.preferred_span,"incomplete default style")
    return {
        min_span=minimum,max_span=maximum,preferred_span=default.preferred_span,
        reserve_width=has_reservation and function(values,style_name)
            local selected=style_name and form.styles[style_name]
            if selected then return selected.reserve_width and selected.reserve_width(values) or nil end
            local reserved=0
            for _,style in form.styles do
                if style.reserve_width then reserved=math.max(reserved,style.reserve_width(values)) end
            end
            return reserved
        end or nil,
        render=function(props,settings,data)
            local instance=props.instance or {}
            local name=instance.style
            if type(name)~="string" or name=="" then
                name=form.legacy_style_setting and settings[form.legacy_style_setting]
            end
            if type(name)~="string" or name=="" then name=form.default_style end
            local selected=form.styles[name]
            assert(selected,"unknown "..placement.." style "..tostring(name).." for "..package.id)
            local styled=table.clone(props)
            styled.form_style=name
            local node
            if package.plugin then
                -- Spec 0203: a failing or over-reaching plugin renders nothing and is switched off.
                local ok,result=pcall(selected.render,styled,settings,data)
                if ok then ok,result=pcall(ui.plugin_gate,package.plugin,result) end
                if not ok then
                    ui.plugin_fault(package.plugin,tostring(result))
                    return ui.Box{id=package.id,width=0,height=0,shrink=0,visible=false}
                end
                node=result
            else
                node=selected.render(styled,settings,data)
            end
            local colour_role=settings.colour_role or package.accent_role
            if node and package.accent_role and colour_role~=package.accent_role then
                node=map_widget_colour(node,package.accent_role,colour_role)
            end
            return node
        end,
    }
end
local function validate_panel_form(package)
    local form=package.forms.panel
    if form==nil then return end
    assert(type(form)=="table" and type(form.styles)=="table","invalid panel forms: "..package.id)
    for name,descriptor in form.styles do
        assert(type(name)=="string" and type(descriptor)=="table" and type(descriptor.view)=="string"
            and #descriptor.view>0,"panel styles only describe a view: "..package.id.."/"..tostring(name))
        for key in descriptor do assert(key=="view","panel descriptors contain view only") end
    end
end
local function install(package,ui)
    assert(type(package.forms)=="table" and package.forms.bar,"missing bar forms: "..package.id)
    for placement in package.forms do
        assert(placement=="bar" or placement=="desktop" or placement=="panel",
            "unsupported widget form placement (only bar, desktop, and panel exist): "..tostring(placement))
    end
    validate_panel_form(package)
    package.variants={bar=make_variant(package,"bar",package.forms.bar,ui)}
    if package.forms.desktop then package.variants.canvas=make_variant(package,"desktop",package.forms.desktop,ui) end
end

M.install=install
return M
