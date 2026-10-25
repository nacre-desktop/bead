-- SPDX-License-Identifier: GPL-3.0-only
-- Shell-owned catalogue and versioned layout. Validation never rewrites the document.
local registry={}
local ids={"launcher","workspaces","title","pill","updates","sound","wifi","bluetooth","battery","notifications","clock","power"}
local function integer(n,lo,hi)
    return type(n)=="number" and n%1==0 and n>=lo and n<=hi
end
local function finite(n,lo,hi)
    return type(n)=="number" and n==n and n>=lo and n<=hi
end
local function settings(schema,values)
    assert(type(values)=="table","widget settings must be an object")
    for key,value in values do
        local field=schema[key]
        -- A newer package may add keys; preserve them without interpreting them.
        if field then
            local kind=field.type
            if kind=="choice" then
                assert(table.find(field.options,value),"invalid widget choice: "..key)
            elseif kind=="toggle" then assert(type(value)=="boolean","invalid widget toggle: "..key)
            elseif kind=="number" or kind=="slider" then
                assert(type(value)=="number" and value==value and value>=field.min and value<=field.max,"invalid widget number: "..key)
            else assert(type(value)=="string" and #value<=4096,"invalid widget text: "..key) end
        end
    end
end
local function make_variant(package,placement,form)
    assert(type(form)=="table" and type(form.styles)=="table" and type(form.default_style)=="string",
        "invalid "..placement.." forms: "..package.id)
    local default=form.styles[form.default_style]
    assert(type(default)=="table","missing default "..placement.." style: "..package.id)
    local minimum,maximum=math.huge,0
    local reserved=0
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
        if style.reserve_width then reserved=math.max(reserved,style.reserve_width(package.defaults)) end
    end
    assert(default.min_span and default.max_span and default.preferred_span,"incomplete default style")
    return {
        min_span=minimum,max_span=maximum,preferred_span=default.preferred_span,
        reserve_width=reserved>0 and function(_) return reserved end or nil,
        render=function(props,settings,data)
            local instance=props.instance or {}
            local name=instance.style or form.default_style
            local selected=form.styles[name]
            assert(selected,"unknown "..placement.." style "..tostring(name).." for "..package.id)
            local styled=table.clone(props)
            styled.form_style=name
            return selected.render(styled,settings,data)
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
local function install_variants(package)
    assert(type(package.forms)=="table" and package.forms.bar,"missing bar forms: "..package.id)
    for placement in package.forms do
        assert(placement=="bar" or placement=="desktop" or placement=="panel",
            "unsupported widget form placement (only bar, desktop, and panel exist): "..tostring(placement))
    end
    validate_panel_form(package)
    package.variants={bar=make_variant(package,"bar",package.forms.bar)}
    if package.forms.desktop then package.variants.canvas=make_variant(package,"desktop",package.forms.desktop) end
end
local function validate_style(package,item,placement,span)
    local form=package.forms[placement]
    assert(form,"widget "..package.id.." has no "..placement.." form")
    local name=item.style or form.default_style
    local style=form.styles[name]
    assert(style,"unknown "..placement.." style "..tostring(name).." for "..package.id)
    if span then
        assert(span>=style.min_span and span<=style.max_span,"widget span outside selected form style")
    end
    return style
end
for _,id in ids do
    local package=require(id.."/widget")
    if table.find({"updates","sound","wifi","bluetooth","battery","notifications","clock","power"},id) then
        package.settings.dropdown_trigger={type="choice",options={"default","click","hover"}}
        package.defaults.dropdown_trigger="default"
    end
    install_variants(package)
    assert(package.id==id and type(package.title)=="string" and type(package.icon)=="string" and type(package.category)=="string","invalid widget identity: "..id)
    assert(type(package.data)=="table" and type(package.data.topics)=="table" and type(package.data.actions)=="table","invalid widget data: "..id)
    assert(type(package.settings)=="table" and type(package.defaults)=="table" and type(package.budget)=="table","invalid widget metadata: "..id)
    assert(integer(package.budget.fps_active,0,120) and integer(package.budget.fps_idle,0,120) and type(package.budget.pause_on)=="table","invalid widget budget: "..id)
    assert(type(package.variants)=="table" and package.variants.bar,"missing bar variant: "..id)
    local kinds={toggle=true,choice=true,slider=true,number=true,color=true,text=true,icon=true,action=true}
    for key,field in package.settings do
        assert(type(key)=="string" and type(field)=="table" and kinds[field.type],"invalid widget settings schema: "..id)
        if field.type=="choice" then
            assert(type(field.options)=="table" and #field.options>0,"invalid widget choices: "..id)
        elseif field.type=="slider" or field.type=="number" then
            assert(type(field.min)=="number" and type(field.max)=="number" and field.min<=field.max,"invalid widget range: "..id)
        end
    end
    settings(package.settings,package.defaults)
    registry[id]=package
end
local M={catalogue=registry,default=require("default")}
function M.validate(document)
    assert(type(document)=="table" and document.version==1,"unsupported widget layout version")
    assert(type(document.preset)=="string" and type(document.hosts)=="table","invalid widget layout")
    assert(document.dropdown_trigger==nil or document.dropdown_trigger=="click" or document.dropdown_trigger=="hover","invalid default dropdown trigger")
    assert(document.hosts.top_bar,"widget layout needs top_bar")
    local seen,count={},0
    local function claim(item,host,placement)
        count+=1
        assert(count<=128 and type(item)=="table","too many widgets")
        assert(type(item.iid)=="string" and #item.iid>0 and #item.iid<=128 and not seen[item.iid],"duplicate or invalid widget iid")
        seen[item.iid]=true
        assert(type(item.widget)=="string" and #item.widget>0 and #item.widget<=128 and item.host==host,"invalid widget host or id")
        local package=registry[item.widget]
        if package then
            settings(package.settings,item.settings or {})
            validate_style(package,item,placement)
            assert(not item.children or item.widget=="pill","only a pill accepts children")
        end
        return package
    end
    local function instances(list,host,tiles,depth)
        assert(depth<=8 and type(list)=="table" and #list<=128,"invalid widget children")
        local occupied={left={},centre={},right={}}
        for _,item in list do
            local package=claim(item,host,"bar")
            local slot=item.slot
            assert(type(slot)=="table" and occupied[slot.region] and integer(slot.start,0,tiles-1) and integer(slot.span,1,tiles) and slot.start+slot.span<=tiles,"invalid widget slot")
            for tile=slot.start,slot.start+slot.span-1 do
                assert(not occupied[slot.region][tile],"overlapping widget slots")
                occupied[slot.region][tile]=true
            end
            if package then validate_style(package,item,"bar",slot.span) end
            -- Unknown packages and their opaque settings/children survive round trips.
            if item.children then instances(item.children,item.iid,tiles,depth+1) end
        end
    end
    local function canvas_instances(list,host,width,height,depth)
        assert(depth<=8 and type(list)=="table" and #list<=128,"invalid canvas children")
        for _,item in list do
            local package=claim(item,host,"desktop")
            local slot=item.slot
            assert(type(slot)=="table" and finite(slot.x,0,20000) and finite(slot.y,0,20000)
                and finite(slot.w,0.000001,20000) and finite(slot.h,0.000001,20000)
                and slot.x+slot.w<=width and slot.y+slot.h<=height,"invalid canvas slot")
            if package then validate_style(package,item,"desktop",slot.w) end
            if item.children then instances(item.children,item.iid,document.hosts.top_bar.tiles,depth+1) end
        end
    end
    for host,layout in document.hosts do
        assert(type(host)=="string" and type(layout)=="table","invalid widget host")
        if host=="canvas" then
            assert(finite(layout.width,1,20000) and finite(layout.height,1,20000),"invalid canvas dimensions")
            canvas_instances(layout.instances,"canvas",layout.width,layout.height,0)
        else
        assert(integer(layout.tiles,1,4096),"invalid widget host")
        assert(layout.mode==nil or layout.mode=="content" or layout.mode=="tiles","invalid widget host mode")
        assert(type(layout.regions)=="table","invalid widget regions")
        if host=="top_bar" then
            assert(layout.regions.left and layout.regions.centre and layout.regions.right,"top_bar needs left/centre/right regions")
        end
        for name,region in layout.regions do
            assert((name=="left" or name=="centre" or name=="right") and type(region)=="table" and integer(region.start,0,layout.tiles-1) and integer(region.span,1,layout.tiles-region.start),"invalid widget region")
        end
        instances(layout.instances,host,layout.tiles,0)
        for _,item in layout.instances do
            local region=layout.regions[item.slot.region]
            assert(region and item.slot.start+item.slot.span<=region.span,"widget slot exceeds region")
        end
        end
    end
    return document
end
function M.renderer(document)
    M.validate(document)
    local copies={}
    local render
    render=function(item,variant)
        local package=registry[item.widget]
        local values=table.clone(package.defaults)
        for key,value in item.settings or {} do values[key]=value end
        local editing=require("context").ui.resolve_for_check(require("context").ui.bind("widgets.editing"))
        local host=document.hosts[item.host]
        local node=variant.render({instance=item,catalogue=registry,render=render,editing=editing,local_layout=host and host.mode=="tiles"},values,{})
        if node.kind~="group" then node.props.drag_payload=item.iid end
        -- Dropdown identity follows the instance, including copies and moves out of pills.
        local function own(child)
            local p=child.props or {}
            local action=p.on_activate
            if action and (action.name=="status.pill.open" or
                (action.name=="view.open" and (item.widget=="clock" or item.widget=="power"))) then
                local trigger=values.dropdown_trigger
                if trigger==nil or trigger=="default" then trigger=document.dropdown_trigger or "click" end
                p.on_activate={name="status.pill.open",argument={{iid=item.iid,view=action.argument,trigger=trigger,host=item.host}}}
            end
            for _,nested in child.children or {} do own(nested) end
        end
        if item.widget~="pill" then own(node) end
        if editing then node=require("editing").decorate(document,item,node) end
        -- Preserve historic IDs for the first copy; additional copies get stable scopes.
        if copies[item.widget] then node.scope="widget."..item.iid.."." end
        copies[item.widget]=true
        return node
    end
    return render
end
return M
