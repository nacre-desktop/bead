-- SPDX-License-Identifier: GPL-3.0-only
-- Shell-owned catalogue and versioned layout. Validation never rewrites the document.
local registry={}
local context=require("context")
local ui,kit=context.ui,context.kit
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
local measured_width=require("measure").width
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
-- Project the shared saved layout onto this output without changing its source.
-- Starts are relative to each region; spans remain tile (about 32 dp) units.
function M.project(document,width)
    assert(type(width)=="number" and width==width and width>0 and width<math.huge,"invalid widget projection width")
    M.validate(document)
    local original=document.hosts.top_bar
    local source_tiles=original.tiles
    local target_tiles=math.max(6,math.min(4096,math.floor(width/32+.5)))
    local content=original.mode=="content"
    local required: {[string]: number}={left=0,centre=0,right=0}
    for _,item in original.instances do
        local region: string=item.slot.region
        local extent: number=item.slot.span
        if content then extent=item.slot.start+item.slot.span end
        if content then
            required[region]=math.max(required[region],extent)
        else
            required[region]+=item.slot.span
        end
    end
    for _,span in required do target_tiles=math.max(target_tiles,span) end
    local old_regions=original.regions
    local projected=table.clone(document)
    projected.hosts=table.clone(document.hosts)
    local source=table.clone(original)
    source.regions=table.clone(original.regions)
    if #original.instances>0 then
        source.instances={}
    end
    for _,item in original.instances do
        local projected_item=table.clone(item)
        projected_item.slot=table.clone(item.slot)
        table.insert(source.instances,projected_item)
    end
    projected.hosts.top_bar=source
    source.tiles=target_tiles
    for name,region in source.regions do
        local old=old_regions[name]
        region=table.clone(region)
        local first=math.floor(old.start*target_tiles/source_tiles+.5)
        local last=math.floor((old.start+old.span)*target_tiles/source_tiles+.5)
        local span=math.max(1,last-first,required[name] or 0)
        assert(span<=target_tiles,"widget region cannot fit output projection")
        region.start=math.max(0,math.min(first,target_tiles-span))
        region.span=span
        source.regions[name]=region
    end
    if not content then
        for name,region in source.regions do
            local old=old_regions[name]
            local ordered={}
            for index,item in source.instances do
                if item.slot.region==name then table.insert(ordered,{item=item,index=index}) end
            end
            table.sort(ordered,function(a,b)
                if a.item.slot.start==b.item.slot.start then return a.index<b.index end
                return a.item.slot.start<b.item.slot.start
            end)
            local tail=0
            for index=#ordered,1,-1 do
                tail+=ordered[index].item.slot.span
                ordered[index].remaining=tail
            end
            local cursor=0
            for _,entry in ordered do
                local slot=entry.item.slot
                local desired=math.floor(slot.start*region.span/old.span+.5)
                local maximum=region.span-entry.remaining
                assert(maximum>=cursor,"widget spans exceed output region capacity")
                local start=math.max(cursor,math.min(desired,maximum))
                slot.start=start
                cursor=start+slot.span
            end
        end
    end
    M.validate(projected)
    return projected
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
        local top_bar=document.hosts.top_bar
        local local_layout=item.host~="canvas" and top_bar and top_bar.mode=="tiles"
        local content_layout=item.host~="canvas" and top_bar and top_bar.mode=="content"
        local form=package.forms[item.host=="canvas" and "desktop" or "bar"]
        local selected=form and item.style
        if form and (type(selected)~="string" or selected=="") then
            selected=form.legacy_style_setting and values[form.legacy_style_setting]
        end
        if form and (type(selected)~="string" or selected=="") then selected=form.default_style end
        local reserved: number?=variant.reserve_width and variant.reserve_width(values,selected)
        local available_width: number?
        if local_layout and item.slot and type(item.slot.span)=="number" then
            local viewport=ui.resolve_for_check(ui.bind("bead.viewport.width")) or 0
            if viewport>0 and top_bar.tiles>0 then available_width=viewport/top_bar.tiles*item.slot.span end
        end
        -- Content rows size from their rendered children; maximum reservations
        -- belong to tiled spans, where neighbours need a stable slot width.
        local node=variant.render({instance=item,catalogue=registry,render=render,editing=editing,
            local_layout=local_layout==true,available_width=available_width},values,{})
        if not node then return nil end
        node=require("dragging").attach(document,item,node)
        if item.widget=="pill" and item.host=="top_bar" and host and host.mode=="content" and node.props then
            local props=node.props
            local natural=measured_width(node,0)
            if natural>0 then props.width=ui.animate("pill-width:"..item.iid,natural,.18,"out-cubic") end
            props.clip_children=true
            node.props=props
        end
        local centered_title=content_layout and item.widget=="title" and item.host=="top_bar"
            and item.slot and item.slot.region=="centre"
        if (local_layout or content_layout and not centered_title)
            and available_width and available_width>0 then
            local natural: number
            if content_layout then
                natural=measured_width(node,0)
            elseif item.widget=="pill" and item.host=="top_bar" then
                natural=available_width
            else
                natural=reserved and reserved>0 and reserved or measured_width(node,0)
            end
            local width=math.min(available_width,natural)
            if width>0 then
                local justify="center"
                if item.slot and item.slot.region=="left" then justify="start"
                elseif item.slot and item.slot.region=="right" then justify="end" end
                node=ui.Row{id="flex-reservation-"..tostring(item.iid),width=width,
                    height=require("context").t.status_height,align="center",justify=justify,clip_children=true,node}
            end
        end
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
        if item.host=="top_bar" and host and host.mode=="tiles" and item.slot then
            local viewport=require("context").ui.resolve_for_check(require("context").ui.bind("bead.viewport.width")) or 0
            local pitch=viewport/math.max(1,host.tiles)
            local target=(item.slot.start or 0)*pitch
            if item.slot.region=="right" then target=-target end
            local props=node.props or {}
            props.translate_x=require("context").ui.animate("widget-slot:"..item.iid,target,.18,"out-cubic")-target
            node.props=props
        end
        if item.drag_placeholder==true and item.host=="top_bar" and host then
            local viewport=require("context").ui.resolve_for_check(require("context").ui.bind("bead.viewport.width")) or 0
            local width=available_width or (viewport/math.max(1,host.tiles)*(item.slot and item.slot.span or 1))
            if width>0 then
                node=ui.Box{id="widget-snap-slot-"..tostring(item.iid),width=width,
                    height=require("context").t.status_height,padding=2,
                    radius=kit.tokens.radii.large,
                    fill=ui.alpha("primary",.12),border_width=kit.tokens.spacing.hairline*1.5,
                    border_color="primary",node}
            end
        end
        -- Preserve historic IDs for the first copy; additional copies get stable scopes.
        if item.drag_placeholder==true then
            node.scope="drag-preview."..item.iid.."."
        else
            if copies[item.widget] then node.scope="widget."..item.iid.."." end
            copies[item.widget]=true
        end
        return node
    end
    return render
end
return M
