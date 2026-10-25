-- SPDX-License-Identifier: GPL-3.0-only
-- Shell-owned catalogue and versioned layout. Validation never rewrites the document.
local registry={}
local ids={"launcher","workspaces","title","pill","updates","sound","wifi","bluetooth","battery","notifications","clock","power"}
local function integer(n,lo,hi)
    return type(n)=="number" and n%1==0 and n>=lo and n<=hi
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
for _,id in ids do
    local package=require(id.."/widget")
    assert(package.id==id and type(package.title)=="string" and type(package.icon)=="string" and type(package.category)=="string","invalid widget identity: "..id)
    assert(type(package.data)=="table" and type(package.data.topics)=="table" and type(package.data.actions)=="table","invalid widget data: "..id)
    assert(type(package.settings)=="table" and type(package.defaults)=="table" and type(package.budget)=="table","invalid widget metadata: "..id)
    assert(integer(package.budget.fps_active,0,120) and integer(package.budget.fps_idle,0,120) and type(package.budget.pause_on)=="table","invalid widget budget: "..id)
    assert(type(package.variants)=="table" and package.variants.bar,"missing bar variant: "..id)
    for _,variant in package.variants do
        assert(type(variant.render)=="function" and integer(variant.min_span,1,4096) and integer(variant.max_span,variant.min_span,4096) and integer(variant.preferred_span,variant.min_span,variant.max_span),"invalid widget variant: "..id)
    end
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
    assert(document.hosts.top_bar,"widget layout needs top_bar")
    local seen,count={},0
    local function instances(list,host,tiles,depth)
        assert(depth<=8 and type(list)=="table" and #list<=128,"invalid widget children")
        local occupied={left={},centre={},right={}}
        for _,item in list do
            count+=1
            assert(count<=128 and type(item)=="table","too many widgets")
            assert(type(item.iid)=="string" and #item.iid>0 and #item.iid<=128 and not seen[item.iid],"duplicate or invalid widget iid")
            seen[item.iid]=true
            assert(type(item.widget)=="string" and #item.widget>0 and #item.widget<=128 and item.host==host,"invalid widget host or id")
            local slot=item.slot
            assert(type(slot)=="table" and occupied[slot.region] and integer(slot.start,0,tiles-1) and integer(slot.span,1,tiles) and slot.start+slot.span<=tiles,"invalid widget slot")
            for tile=slot.start,slot.start+slot.span-1 do
                assert(not occupied[slot.region][tile],"overlapping widget slots")
                occupied[slot.region][tile]=true
            end
            local package=registry[item.widget]
            if package then
                local variant=package.variants.bar
                assert(slot.span>=variant.min_span and slot.span<=variant.max_span,"widget span outside variant contract")
                settings(package.settings,item.settings or {})
                assert(not item.children or item.widget=="pill","only a pill accepts children")
            end
            -- Unknown packages and their opaque settings/children survive round trips.
            if item.children then instances(item.children,item.iid,tiles,depth+1) end
        end
    end
    for host,layout in document.hosts do
        assert(type(host)=="string" and type(layout)=="table" and integer(layout.tiles,1,4096),"invalid widget host")
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
        local node=variant.render({instance=item,catalogue=registry,render=render},values,{})
        -- Preserve historic IDs for the first copy; additional copies get stable scopes.
        if copies[item.widget] then node.scope="widget."..item.iid.."." end
        copies[item.widget]=true
        return node
    end
    return render
end
return M
