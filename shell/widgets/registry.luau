-- SPDX-License-Identifier: GPL-3.0-only
-- Shell-owned catalogue and versioned layout. Validation never rewrites the document.
local registry={}
local context=require("context")
local ui,kit=context.ui,context.kit
local ids={"launcher","workspaces","title","pill","sound","wifi","bluetooth","battery","notifications","clock","power"}
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
local function make_variant(package,placement,form)
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
local measured_width=require("measure").width
local translations={
    battery={title="bead.widgets.battery-title",description="bead.widgets.battery-description"},
    bluetooth={title="bead.widgets.bluetooth-title",description="bead.widgets.bluetooth-description"},
    clock={title="bead.widgets.clock-title",description="bead.widgets.clock-description"},
    launcher={title="bead.widgets.launcher-title",description="bead.widgets.launcher-description"},
    notifications={title="bead.widgets.notifications-title",description="bead.widgets.notifications-description"},
    pill={title="bead.widgets.pill-title",description="bead.widgets.pill-description"},
    power={title="bead.widgets.power-title",description="bead.widgets.power-description"},
    sound={title="bead.widgets.sound-title",description="bead.widgets.sound-description"},
    title={title="bead.widgets.window-title",description="bead.widgets.window-title-description"},
    wifi={title="bead.widgets.wifi-title",description="bead.widgets.wifi-description"},
    workspaces={title="bead.widgets.workspaces-title",description="bead.widgets.workspaces-description"},
}
local function validate_style(package,item,placement,span)
    local form=package.forms[placement]
    assert(form,"widget "..package.id.." has no "..placement.." form")
    local name=item.style
    if type(name)~="string" or name=="" then
        name=form.legacy_style_setting and (item.settings or {})[form.legacy_style_setting]
    end
    if type(name)~="string" or name=="" then name=form.default_style end
    local style=form.styles[name]
    assert(style,"unknown "..placement.." style "..tostring(name).." for "..package.id)
    if span then
        assert(span>=style.min_span and span<=style.max_span,"widget span outside selected form style")
    end
    return style
end
local function install(id,package,accent)
    package.accent_role=accent
    package.settings.colour_role={type="choice",options={"primary","secondary","tertiary","neutral"},
        segmented=true,label=ui.tr("bead.widgets.colour")}
    package.defaults.colour_role=package.accent_role
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
for _,id in ids do
    local package=require(id.."/widget")
    package.title=ui.tr(translations[id].title)
    package.description=ui.tr(translations[id].description)
    install(id,package,({clock="tertiary",sound="secondary",wifi="secondary"})[id] or "primary")
end
-- Plugins (spec 0203). The generated index lists every discovered plugin; active ones
-- register here like built-ins. A plugin that fails to load is skipped and switched off.
local found,index=pcall(function() return require("../plugins/index") end)
local plugins=found and index.plugins or {}
local app_api={context=context,measure=require("measure"),visual=require("desktop-visuals")}
local plugin_widgets={}
for _,entry in plugins do
    if not entry.loaded then continue end
    for _,widget in entry.widgets do
        local ok,problem=pcall(function()
            local factory=require("../"..widget.module)
            assert(type(factory)=="function","plugin entry must return a factory function")
            local package=factory({ui=ui,kit=kit,plugin={id=entry.id,settings=table.clone(entry.values)},app=app_api})
            assert(type(package)=="table" and package.id==widget.id,"plugin widget must have id "..widget.id)
            assert(registry[widget.id]==nil,"widget id "..widget.id.." is already registered")
            package.plugin=entry.id
            package.title=widget.title_id~="" and ui.tr(widget.title_id) or entry.name
            package.description=widget.description_id~="" and ui.tr(widget.description_id) or entry.description
            package.settings=package.settings or {}
            package.defaults=package.defaults or {}
            for key,field in entry.settings or {} do
                package.settings[key]={type=field.type,label=field.label,options=field.options,min=field.min,max=field.max,step=field.step}
                package.defaults[key]=entry.values[key]
            end
            install(widget.id,package,widget.accent)
            table.insert(plugin_widgets,widget.id)
        end)
        if not ok then
            entry.loaded=false
            entry.error=tostring(problem)
            if registry[widget.id] and registry[widget.id].plugin==entry.id then registry[widget.id]=nil end
            ui.plugin_fault(entry.id,entry.error)
        end
    end
end
local M={catalogue=registry,default=require("default"),
    plugins={list=plugins,widgets=plugin_widgets,notice=found and index.notice or "",user_dir=found and index.user_dir or false}}
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
            assert(type(slot)=="table" and occupied[slot.region] and integer(slot.start,0,tiles-1) and integer(slot.span,1,tiles) and slot.start+slot.span<=tiles
                and (slot.offset==nil or depth==0 and host=="top_bar" and finite(slot.offset,-20000,20000)),"invalid widget slot")
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
-- B0198: the bar is order-based. Any saved layout (including one broken by an
-- older drag) is repaired into one canonical form: natural content packing, full
-- regions, contiguous starts in saved order and spans from the selected form.
local REGION_RANK={left=1,centre=2,right=3}
local function form_span(item)
    local package=registry[item.widget]
    local form=package and package.forms[item.host=="canvas" and "desktop" or "bar"]
    if not form then return math.max(1,math.floor(tonumber(item.slot and item.slot.span) or 1)) end
    local name=item.style
    if type(name)~="string" or not form.styles[name] then name=form.default_style end
    return form.styles[name].min_span
end
local function pack(list,top)
    local order={}
    for index,item in list do
        if type(item)=="table" and type(item.slot)=="table" then
            if not REGION_RANK[item.slot.region] then item.slot.region="centre" end
            table.insert(order,{item=item,index=index,start=tonumber(item.slot.start) or 0})
        end
    end
    table.sort(order,function(a,b)
        local ar,br=REGION_RANK[a.item.slot.region],REGION_RANK[b.item.slot.region]
        if ar~=br then return ar<br end
        if a.start~=b.start then return a.start<b.start end
        return a.index<b.index
    end)
    local cursor={left=0,centre=0,right=0}
    table.clear(list)
    for _,entry in order do
        local item,region=entry.item,entry.item.slot.region
        item.slot.start=cursor[region]
        item.slot.span=form_span(item)
        item.slot.gap=nil
        local offset=tonumber(item.slot.offset)
        item.slot.offset=top and offset and offset==offset and math.abs(offset)<20000 and math.round(offset) or nil
        cursor[region]+=item.slot.span
        -- Only containers keep children (unknown packages keep theirs opaque).
        if type(item.children)~="table" or (registry[item.widget] and item.widget~="pill") then item.children=nil end
        if item.children then pack(item.children,false) end
        table.insert(list,item)
    end
end
local function copy(value)
    if type(value)~="table" then return value end
    local result={}
    for key,item in value do result[key]=copy(item) end
    return result
end
-- Documents this module already normalised (the bar normalises once per build).
local normalised=setmetatable({},{__mode="k"})
function M.normalise(document)
    if normalised[document] then return document end
    if type(document)~="table" or type(document.hosts)~="table" or type(document.hosts.top_bar)~="table" then return document end
    local doc=copy(document)
    local bar=doc.hosts.top_bar
    local tiles=integer(bar.tiles,6,4096) and bar.tiles or 60
    bar.tiles=tiles
    bar.mode="content"
    bar.regions={left={start=0,span=tiles},centre={start=0,span=tiles},right={start=0,span=tiles}}
    if type(bar.instances)=="table" then pack(bar.instances,true) end
    local canvas=doc.hosts.canvas
    if type(canvas)=="table" and type(canvas.instances)=="table" then
        for _,item in canvas.instances do
            if type(item)=="table" and (type(item.children)~="table" or item.widget~="pill") then item.children=nil end
            local slot=type(item)=="table" and item.slot
            if type(slot)=="table" and finite(canvas.width,1,20000) and finite(canvas.height,1,20000)
                and finite(slot.w,0.000001,canvas.width) and finite(slot.h,0.000001,canvas.height) then
                slot.x=math.clamp(tonumber(slot.x) or 0,0,canvas.width-slot.w)
                slot.y=math.clamp(tonumber(slot.y) or 0,0,canvas.height-slot.h)
            end
        end
    end
    -- Saved bar placements mirror the live item, or keep a form-sized slot for later.
    local live={}
    local function index(items)
        if type(items)~="table" then return end
        for _,item in items do
            if type(item)=="table" then live[item.group or item.iid]=item; index(item.children) end
        end
    end
    if type(bar.instances)=="table" then index(bar.instances) end
    for group,state in type(doc.widgets)=="table" and doc.widgets or {} do
        local form=type(state)=="table" and type(state.forms)=="table" and state.forms.bar or nil
        local placement=type(form)=="table" and form.placement
        local item=live[group]
        if type(placement)=="table" then
            if item and item.host~="canvas" then
                form.placement={host=item.host,slot=copy(item.slot)}
            elseif type(placement.slot)=="table" then
                placement.slot.span=1
            end
        end
    end
    normalised[doc]=true
    return doc
end
-- Project the shared saved layout onto this output without changing its source.
-- Content rows pack naturally; spans only need to fit the output's tiles.
function M.project(document,width)
    assert(type(width)=="number" and width==width and width>0 and width<math.huge,"invalid widget projection width")
    local projected=M.normalise(document)
    local bar=projected.hosts.top_bar
    local tiles=math.max(6,math.min(4096,math.floor(width/32+.5)))
    for _,item in bar.instances do tiles=math.max(tiles,item.slot.start+item.slot.span) end
    bar.tiles=tiles
    for _,region in bar.regions do region.start=0; region.span=tiles end
    M.validate(projected)
    return projected
end
-- B0211: placement along the bar. A widget whose slot has an `offset` stands exactly there:
-- `region` is its anchor (left/right: px from that output edge to its near edge; centre: px from
-- the output centre to its own centre). Widgets without one keep the packed row of their side,
-- so saved layouts load with unchanged pixels. The title stays at the true centre: placed
-- neighbours only narrow it, and older centre neighbours sit beside its widest band.
function M.bar_regions(layout)
    local regions={}
    for name,region in layout.regions do regions[name]=table.clone(region) end
    regions.left.props={id="left",height=34,gap=12,shrink=1,align="center"}
    regions.centre.inline=true
    regions.right.props={id="right",height=34,gap=12,shrink=0,align="center"}
    return regions
end
local function title_reserve(item)
    local package=registry[item.widget]
    local style=package and package.forms.bar.styles[item.style or package.forms.bar.default_style]
    local settings=table.clone(package and package.defaults or {})
    for key,value in item.settings or {} do settings[key]=value end
    return style and style.reserve_width and style.reserve_width(settings) or 0
end
-- Splits `layout.instances` into the Host's packed rows and absolutely placed nodes.
function M.bar_layout(layout,render,width)
    local gap=context.t.status_gap or 12
    local flow,placed,before,after={},{},{},{}
    local title
    for _,item in layout.instances do
        if item.widget=="title" and item.slot.region=="centre" and item.slot.offset==nil then title=item end
    end
    for _,item in layout.instances do
        if type(item.slot.offset)=="number" then table.insert(placed,item)
        elseif item.slot.region=="centre" and item~=title then
            table.insert(title and item.slot.start<title.slot.start and before or after,item)
        else table.insert(flow,item) end
    end
    local nodes={}
    local room=math.huge
    local function variant(item)
        local package=registry[item.widget]
        return package and package.variants.bar
    end
    for _,item in placed do
        local chosen=variant(item)
        local node=chosen and render(item,chosen)
        if node then
            local offset,w=item.slot.offset,measured_width(node,0)
            local box={id="placed-"..item.iid,position="absolute",top=0,height="100%",align="center",node}
            if item.slot.region=="left" then
                box.left=offset
                room=math.min(room,width/2-(offset+w))
            elseif item.slot.region=="right" then
                box.right=offset
                room=math.min(room,width/2-(offset+w))
            else
                box.left=0; box.width=width; box.justify="center"
                box.padding_left=math.max(0,2*offset); box.padding_right=math.max(0,-2*offset)
                room=math.min(room,math.abs(offset)-w/2)
            end
            table.insert(nodes,ui.Row(box))
        end
    end
    local reserve=title and title_reserve(title) or 0
    local half=reserve/2+gap
    for _,side in {{"before",before},{"after",after}} do
        if #side[2]>0 then
            local row={id="beside-title-"..side[1],position="absolute",top=0,height="100%",align="center",gap=gap}
            if side[1]=="before" then row.right=width/2+half else row.left=width/2+half end
            for _,item in side[2] do
                local chosen=variant(item)
                local node=chosen and render(item,chosen)
                if node then table.insert(row,node) end
            end
            table.insert(nodes,ui.Row(row))
        end
    end
    -- Placed neighbours narrow the title (only when they reach its widest band) instead of moving it.
    room=2*(room-gap)
    layout.title_room=room<reserve and math.max(0,room) or nil
    return flow,nodes
end
function M.renderer(document)
    M.validate(document)
    local copies={}
    local bar_layout=document.hosts.top_bar
    local render
    render=function(item,variant)
        local package=registry[item.widget]
        local values=table.clone(package.defaults)
        for key,value in item.settings or {} do values[key]=value end
        local editing=require("context").ui.resolve_for_check(require("context").ui.bind("widgets.editing"))
        local host=document.hosts[item.host]
        local node=variant.render({instance=item,catalogue=registry,render=render,editing=editing,
            local_layout=item.host=="top_bar" and (item.slot.offset~=nil or item.slot.region=="centre" and item.widget~="title"),
            available_width=item.widget=="title" and bar_layout and bar_layout.title_room or nil},values,{})
        if not node then return nil end
        node=require("dragging").attach(document,item,node)
        if item.widget=="pill" and item.host=="top_bar" and host and node.props then
            local props=node.props
            -- An empty pill stays a visible drop target as wide as it is tall.
            local natural=math.max(measured_width(node,0),require("context").t.pill_height)
            -- B0198: set, not animated: a right-anchored pill shrinking over several frames
            -- left stale copies of itself in the bar (partial damage) mid-drag.
            if natural>0 then props.width=natural end
            props.clip_children=true
            node.props=props
        end
        -- B0208 edit mode: a click on a placed widget opens that instance's settings in the
        -- Widgets app instead of its dropdown or action.
        local inspect=editing and item.widget~="pill" and require("editing").inspect(item) or nil
        -- Dropdown identity follows the instance, including copies and moves out of pills.
        local function own(child)
            local p=child.props or {}
            local action=p.on_activate
            if inspect and action then
                p.on_activate=inspect
            elseif action and (action.name=="status.pill.open" or
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
