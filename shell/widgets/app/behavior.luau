-- SPDX-License-Identifier: GPL-3.0-only
-- On-demand Widgets app model. The package registry remains the authority at stage time.
local LIMIT=8
local function clone(value) return copy_value(value) end
local function request(args)
    local value=args and args[1]
    return type(value)=="table" and value or nil
end
local function document(s,req)
    return clone((s["widgets.draft"] or {})[1] or req.document or (s["widgets.layout"] or {})[1])
end
local function push_history(s,old)
    local undo=clone(s["widgets.undo"] or {})
    if old then
        table.insert(undo,{document=clone(old)})
        while #undo>LIMIT do table.remove(undo,1) end
    end
    s["widgets.undo"]=undo
end
local function history(s,doc)
    push_history(s,(s["widgets.draft"] or {})[1] or doc)
end
local function parent_history(s,args)
    local req=request(args)
    if not req or type(req.previous_document)~="table" or type(req.document)~="table" then return end
    push_history(s,req.previous_document)
    s["widgets.layout"]={clone(req.document)}
    s["widgets.draft"]={}
    s["widgets.rev"]=req.revision
    s["widgets.history.revision"]=req.revision
    s["widgets.busy"]=false
    s["widgets.error"]=""
    s["widgets.message"]="Changes saved"
end
local function finish(s,doc)
    s["widgets.draft"]={doc}
    s["widgets.error"]=""
    s["widgets.message"]=""
end
local function catalogue(req,id)
    for _,item in req.catalogue or {} do if item.id==id then return item end end
end
local function group_for(doc,id,preferred,req)
    if type(preferred)=="string" and preferred~="" and doc.widgets and doc.widgets[preferred] then
        local state=doc.widgets[preferred]
        assert(state.widget==id,"widget group belongs to another package")
        return preferred,state
    end
    local matching_groups={}
    if type(preferred)~="string" or preferred=="" then
        for group,item in doc.widgets or {} do if item.widget==id then table.insert(matching_groups,{group=group,state=item}) end end
        if #matching_groups>1 then error("widget group must identify one of the configured instances") end
        if #matching_groups==1 then return matching_groups[1].group,matching_groups[1].state end
    end
    local matches={}
    local function scan(items)
        for _,item in items or {} do
            if item.widget==id and (not preferred or item.group==preferred or item.iid==preferred) then table.insert(matches,item) end
            scan(item.children)
        end
    end
    for _,host in doc.hosts or {} do scan(host.instances) end
    if not preferred then
        local groups={}
        for _,item in matches do groups[item.group or item.iid]=true end
        local count=0
        for _ in groups do count+=1 end
        if count>1 then error("widget group is required when a package has multiple instances") end
    end
    local found=matches[1]
    local group=preferred or (found and (found.group or found.iid)) or "widget-"..id
    local state=doc.widgets and doc.widgets[group]
    if state then assert(state.widget==id,"widget group belongs to another package"); return group,state end
    if not state then
        state={widget=id,settings=clone((found and found.settings) or (req and req.settings) or {}),forms={}}
        for _,form in {"bar","desktop"} do
            state.forms[form]={enabled=false,style="",placement={}}
        end
        if found then
            found.group=group
            local placement=found.host=="canvas" and "desktop" or "bar"
            local dest=state.forms[placement]
            dest.enabled=true; dest.style=found.style or ""; dest.placement={host=found.host,slot=clone(found.slot)}
        end
        doc.widgets=doc.widgets or {}
        doc.widgets[group]=state
    end
    if preferred and not found and req and type(req.settings)=="table" then state.settings=clone(req.settings) end
    return group,state
end
local function each_list(doc,callback)
    local function visit(items)
        for _,item in items or {} do
            callback(item)
            visit(item.children)
        end
    end
    for _,host in doc.hosts or {} do visit(host.instances) end
end
local function group_items(doc,group,id)
    local result={}
    each_list(doc,function(item)
        if item.group==group or (item.group==nil and item.widget==id and item.iid==group) then table.insert(result,item) end
    end)
    return result
end
local function remove_identity(items,iid)
    for index=#items,1,-1 do
        if items[index].iid==iid then return table.remove(items,index) end
        local found=remove_identity(items[index].children or {},iid)
        if found then return found end
    end
end
local function locate_identity(items,iid,host,parent)
    for index,item in items or {} do
        if item.iid==iid then return {item=item,index=index,host=host,parent=parent} end
        local found=locate_identity(item.children,iid,item.iid,item.iid)
        if found then return found end
    end
end
local function document_location(doc,location)
    if location.parent then
        local found=locate_identity(doc.hosts.top_bar.instances,location.parent,"top_bar")
        return found and found.item.children or nil
    end
    local host=doc.hosts[location.host]
    return host and host.instances or nil
end
local function locate_document_item(doc,iid)
    for host,layout in doc.hosts or {} do
        local found=locate_identity(layout.instances,iid,host)
        if found then return found end
    end
end
local function output_dimensions(doc,event,context)
    local canvas=doc.hosts.canvas or {}
    local function dimension(value)
        local number=tonumber(value)
        if number and number==number and number>=1 and number<=20000 then return number end
    end
    return dimension(event.screen_width) or dimension(context.screen_width) or dimension(context.canvas_width)
            or dimension(canvas.width) or 1920,
        dimension(event.screen_height) or dimension(context.screen_height) or dimension(context.canvas_height)
            or dimension(canvas.height) or 1080
end
local function update_setting(doc,group,state,key,value)
    state.settings=state.settings or {}
    state.settings[key]=clone(value)
    for _,item in group_items(doc,group,state.widget) do
        item.settings=item.settings or {}
        item.settings[key]=clone(value)
    end
end
local function drag_record(value)
    if type(value)=="table" and value[1] then return value[1] end
    return value
end
local function drag_clear(s)
    s["widgets.drag.active"]=false
    s["widgets.drag.source"]={}
    s["widgets.drag.destination"]=""
    s["widgets.drag.phase"]=""
    s["widgets.drag.base"]={}
    s["widgets.drag.original"]={}
    s["widgets.drag.layout"]={}
    s["widgets.drag.revision"]=0
    s["widgets.drag.entry.target_width"]=0
    s["widgets.drag.entry.target_height"]=0
end
local function current_canonical(s,context)
    local saved=(s["widgets.layout"] or {})[1]
    if type(saved)=="table" then return clone(saved) end
    local materialized=type(context)=="table" and (context.document or context.initial) or nil
    return clone(materialized or (s["widgets.initial"] or {})[1] or {})
end
local function drag_revision_stale(s)
    return (tonumber(s["widgets.drag.revision"]) or 0)~=(tonumber(s["widgets.rev"]) or 0)
end
local function return_form(source)
    if type(source)=="table" and (source.form=="bar" or source.form=="desktop") then return source.form end
    return type(source)=="table" and source.host=="canvas" and "desktop" or "bar"
end
local function output_local(event,context)
    if type(event.destination_x)=="number" and type(event.destination_y)=="number" then
        return event.destination_x,event.destination_y
    end
    local origin_x=tonumber(event.output_origin_x or context.screen_origin_x) or 0
    local origin_y=tonumber(event.output_origin_y or context.screen_origin_y) or 0
    local gx=tonumber(event.global_x) or ((tonumber(event.x) or 0)+(tonumber(event.source_origin_x) or 0))
    local gy=tonumber(event.global_y) or ((tonumber(event.y) or 0)+(tonumber(event.source_origin_y) or 0))
    return gx-origin_x,gy-origin_y
end
local function contains_identity(item,iid)
    if item.iid==iid then return true end
    for _,child in item.children or {} do if contains_identity(child,iid) then return true end end
    return false
end
local function pill_at_pointer(doc,event,context)
    local explicit=drag_record(event.target)
    local iid=type(explicit)=="table" and (explicit.iid or (type(explicit.target)=="table" and explicit.target.iid)) or nil
    if type(iid)=="string" then
        local found=locate_document_item(doc,iid)
        if found and found.item.widget=="pill" and found.host=="top_bar" and not found.parent then return found.item end
    end
    local x,y=output_local(event,context)
    for _,rect in context.geometry or {} do
        if type(rect.iid)=="string" and type(rect.x)=="number" and type(rect.y)=="number"
            and type(rect.w)=="number" and type(rect.h)=="number"
            and x>=rect.x and y>=rect.y and x<rect.x+rect.w and y<rect.y+rect.h then
            local found=locate_document_item(doc,rect.iid)
            if found and found.item.widget=="pill" and found.host=="top_bar" and not found.parent then return found.item end
        end
    end
end
-- B0198: the bar is order-based (content packing). Every committed or previewed
-- document is normalised: per list and region the saved order is kept, starts are
-- contiguous and spans come from the selected form, so no drag can leave overlaps,
-- gaps or inflated spans behind. Mirrors registry.normalise for the markup side.
local REGION_RANK={left=1,centre=2,right=3}
local function form_span(req,item)
    local package=catalogue(req,item.widget)
    local form=package and package.forms and package.forms[item.host=="canvas" and "desktop" or "bar"]
    local styles=form and form.styles or {}
    local style=styles[item.style or ""] or styles[form and form.default_style or ""]
    return style and tonumber(style.min_span) or math.max(1,math.floor(tonumber(item.slot and item.slot.span) or 1))
end
local function pack_list(req,list)
    local order={}
    for index,item in list do
        if type(item.slot)~="table" then item.slot={} end
        if not REGION_RANK[item.slot.region] then item.slot.region="centre" end
        table.insert(order,{item=item,index=index,start=tonumber(item.slot.start) or 0})
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
        item.slot={region=region,start=cursor[region],span=form_span(req,item)}
        cursor[region]+=item.slot.span
        if type(item.children)~="table" or (catalogue(req,item.widget) and item.widget~="pill") then item.children=nil end
        if item.children then pack_list(req,item.children) end
        table.insert(list,item)
    end
end
local function normalise(doc,req)
    local bar=doc.hosts.top_bar
    local tiles=tonumber(bar.tiles) or 60
    if tiles<6 or tiles%1~=0 then tiles=60 end
    bar.tiles=tiles
    bar.mode="content"
    bar.regions={left={start=0,span=tiles},centre={start=0,span=tiles},right={start=0,span=tiles}}
    pack_list(req,bar.instances)
    local live={}
    local seen,unique={},true
    local function index(items)
        for _,item in items or {} do
            if seen[item.iid] then unique=false end
            seen[item.iid]=true
            live[item.group or item.iid]=item
            index(item.children)
        end
    end
    for _,host in doc.hosts do index(host.instances) end
    for group,state in doc.widgets or {} do
        local form=type(state.forms)=="table" and state.forms.bar
        local item=live[group]
        if type(form)=="table" and item and item.host~="canvas" then
            form.placement={host=item.host,slot=clone(item.slot)}
        elseif type(form)=="table" and type(form.placement)=="table" and type(form.placement.slot)=="table" then
            form.placement.slot.span=1
        end
    end
    return unique
end
local function remove_located(doc,location)
    local list=document_location(doc,location)
    if not list then return nil end
    return table.remove(list,location.index)
end
local function centre_x(context,iid)
    for _,rect in context.geometry or {} do
        if rect.iid==iid and type(rect.x)=="number" and type(rect.w)=="number" then return rect.x+rect.w/2 end
    end
end
-- Insert by pointer order among the measured siblings; ties and unmeasured rows keep order.
local function insert_ordered(list,item,x,region,context,regional)
    local rows={}
    for index,other in list do
        if not regional or other.slot.region==region then table.insert(rows,{item=other,index=index}) end
    end
    table.sort(rows,function(a,b)
        local ar,br=REGION_RANK[a.item.slot.region],REGION_RANK[b.item.slot.region]
        if ar~=br then return ar<br end
        if a.item.slot.start~=b.item.slot.start then return a.item.slot.start<b.item.slot.start end
        return a.index<b.index
    end)
    local before,after,measured,last
    for _,row in rows do
        local cx=centre_x(context,row.item.iid)
        last=row.item
        if cx then
            measured=true
            if x<cx then after=row.item; break end
            before=row.item
        end
    end
    if not measured then before,after=last,nil end
    -- Between two regions (inside a pill) the nearer neighbour's region wins; ties join the next.
    local join_after=after and (regional or not before or before.slot.region==after.slot.region
        or (centre_x(context,after.iid) or x)-x<=x-(centre_x(context,before.iid) or x))
    if join_after then
        item.slot={region=after.slot.region,start=after.slot.start-.5,span=1}
    elseif before then
        item.slot={region=before.slot.region,start=before.slot.start+.5,span=1}
    else
        item.slot={region=region,start=math.huge,span=1}
    end
    table.insert(list,item)
end
local function promote_children(doc,pill)
    for index,child in pill.children or {} do
        child.host="top_bar"
        child.slot={region=pill.slot.region,start=pill.slot.start+index/1000,span=1}
        table.insert(doc.hosts.top_bar.instances,child)
    end
    pill.children={}
end
-- The removal zone shown by the drag overlay; geometry is derived from the output size.
local function in_remove_zone(event,context)
    local zone=context.remove_zone
    if type(zone)~="table" then return false end
    local width,height=tonumber(event.screen_width) or tonumber(context.screen_width) or 0,tonumber(event.screen_height) or tonumber(context.screen_height) or 0
    local x,y=output_local(event,context)
    local w,h=tonumber(zone.width) or 0,tonumber(zone.height) or 0
    local left,top=(width-w)/2,height-(tonumber(zone.bottom) or 0)-h
    return w>0 and h>0 and x>=left and x<left+w and y>=top and y<top+h
end
local function remove_instance(doc,group_state,location)
    local item=remove_located(doc,location)
    if not item then return false end
    if item.widget=="pill" and item.children and #item.children>0 then
        item.slot=item.slot or {region="right",start=0,span=1}
        promote_children(doc,item)
    end
    local form=group_state and group_state.forms and group_state.forms[item.host=="canvas" and "desktop" or "bar"]
    if form then form.enabled=false; form.placement={host=item.host,slot=clone(item.slot)} end
    return true,item
end
-- Resize previews share the transfer transaction and never change placement form.
local function drag_resize(s,event,context,base,commit,undo_document,source)
    if source.app==true or source.host~="canvas" or type(source.iid)~="string" then return false end
    local doc=type(base)=="table" and clone(base) or document(s,context)
    if type(doc)~="table" then return false end
    local location=locate_document_item(doc,source.iid)
    if not location or location.host~="canvas" or location.item.widget~=source.widget then return false end
    local item=location.item
    local canvas=doc.hosts.canvas
    local package=catalogue(context,source.widget)
    local form=package and package.forms and package.forms.desktop
    local style=form and form.styles[item.style or source.style or form.default_style]
    if not style then return false end
    local dx=(tonumber(event.x) or 0)-(tonumber(event.start_x) or 0)
    local dy=(tonumber(event.y) or 0)-(tonumber(event.start_y) or 0)
    if dx~=dx or dy~=dy or math.abs(dx)>20000 or math.abs(dy)>20000 then return false end
    local slot=item.slot
    local maximum_w=math.min(style.max_span,canvas.width-slot.x)
    local maximum_h=math.min(tonumber(source.max_height) or canvas.height,canvas.height-slot.y)
    local minimum_w=math.min(maximum_w,style.min_span)
    local minimum_h=math.min(maximum_h,math.max(1,tonumber(source.min_height) or 1))
    slot.w=math.clamp(slot.w+dx,minimum_w,maximum_w)
    slot.h=math.clamp(slot.h+dy,minimum_h,maximum_h)
    local group,state=group_for(doc,item.widget,item.group or item.iid,context)
    item.group=group
    local config=state.forms.desktop or {enabled=true,style=item.style or form.default_style}
    state.forms.desktop=config
    config.placement={host="canvas",slot=clone(slot)}
    if commit~=false then
        history(s,undo_document or base); finish(s,doc)
        s["widgets.write"]={clone(doc)}
    end
    return true,doc,item
end
local function commit_drop(s,doc,previous,commit)
    if commit~=false then
        history(s,previous); finish(s,doc)
        s["widgets.write"]={clone(doc)}
    end
end
local function unique_iid(doc,stem)
    local iid,suffix=stem,1
    while locate_document_item(doc,iid) do suffix+=1; iid=stem..":"..tostring(suffix) end
    return iid
end
-- One placement transaction: detach, place by pointer order, normalise, validate.
-- Any rejection leaves the base untouched so the ghost snaps back.
local function drag_drop(s,event,context,base,commit,undo_document)
    local source=drag_record(event.source)
    if type(source)~="table" or type(source.widget)~="string" then return false end
    if source.kind=="resize" then return drag_resize(s,event,context,base,commit,undo_document,source) end
    if source.app==true and event.private_remote~=true then return false end
    local destination=event.destination
    if destination~="bar" and destination~="desktop" then return false end
    if source.app==true and event.source_origin_valid~=true then return false end
    local id=source.widget
    local req={catalogue=context.catalogue,settings=source.settings,group=source.group}
    local doc=type(base)=="table" and clone(base) or document(s,context)
    if type(doc)~="table" or type(doc.hosts)~="table" or type(doc.hosts.top_bar)~="table" then return false end
    local previous=undo_document or clone(doc)
    if not normalise(doc,req) then return false end
    local live_location=type(source.iid)=="string" and locate_document_item(doc,source.iid) or nil
    if live_location and live_location.item.widget~=id then return false end
    local group,state=group_for(doc,id,source.group or source.iid or id,req)
    if source.app~=true and live_location and destination=="desktop" and in_remove_zone(event,context) then
        if not remove_instance(doc,state,live_location) or not normalise(doc,req) then return false end
        commit_drop(s,doc,previous,commit)
        return true,doc,nil
    end
    local package=catalogue(req,id)
    local form=destination=="bar" and "bar" or "desktop"
    local descriptor=package and package.forms and package.forms[form]
    if not descriptor then return false end
    local config=state.forms[form] or {enabled=false,style=descriptor.default_style,placement={}}
    state.forms[form]=config
    local style_name=config.style
    if type(style_name)~="string" or not descriptor.styles[style_name] then
        local legacy=descriptor.legacy_style_setting and source.settings and source.settings[descriptor.legacy_style_setting]
        style_name=descriptor.styles[source.style] and source.style or (descriptor.styles[legacy] and legacy) or descriptor.default_style
    end
    local style=descriptor.styles[style_name]
    if not style then return false end
    local source_form=live_location and (live_location.item.host=="canvas" and "desktop" or "bar") or nil
    -- A live source moves; an app source reuses the group's instance in this form, if any.
    local location=live_location
    if not location or source_form~=form then
        for _,other in group_items(doc,group,id) do
            if (other.host=="canvas")==(form=="desktop") then location=locate_document_item(doc,other.iid); break end
        end
        if location and location.item==(live_location and live_location.item) then location=nil end
    end
    local item
    if location then
        item=remove_located(doc,location)
    elseif live_location then
        -- Morph: the live instance leaves its old form, which keeps its slot for later.
        local removed
        removed,item=remove_instance(doc,state,live_location)
        if not removed then return false end
        item.children=nil
    else
        item={iid=unique_iid(doc,form=="desktop" and group..":desktop" or group),widget=id,group=group,
            settings=clone(state.settings or source.settings or package.defaults or {})}
    end
    item.group=group; item.style=style_name
    item.settings=item.settings or {}
    for key,value in source.settings or {} do item.settings[key]=clone(value) end
    if form=="desktop" then
        local width,height=output_dimensions(doc,event,context)
        local canvas=doc.hosts.canvas or {instances={}}
        doc.hosts.canvas=canvas
        canvas.width=width; canvas.height=height
        local x,y=output_local(event,context)
        local configured=config.placement and config.placement.slot or {}
        local previous_slot=item.host=="canvas" and type(item.slot)=="table" and item.slot or {}
        local w=tonumber(configured.w) or tonumber(previous_slot.w) or style.preferred_span
        local h=tonumber(configured.h) or tonumber(previous_slot.h) or style.preferred_height or 96
        w=math.clamp(w,style.min_span,style.max_span)
        if w>width or h<=0 or h>height then return false end
        x-=(tonumber(event.grab_x) or 0)*w/math.max(1,tonumber(event.width) or w)
        y-=(tonumber(event.grab_y) or 0)*h/math.max(1,tonumber(event.height) or h)
        local function magnet(value,extent)
            value=math.clamp(value,0,extent)
            local nearest=value
            local distance=tonumber(context.snap_distance) or 16
            for _,target in {0,extent/2,extent} do
                if math.abs(value-target)<distance then nearest=target; distance=math.abs(value-target) end
            end
            return nearest
        end
        item.host="canvas"
        item.children=nil
        item.slot={x=magnet(tonumber(event.slot and event.slot.x) or x,width-w),
            y=magnet(tonumber(event.slot and event.slot.y) or y,height-h),w=w,h=h}
        table.insert(canvas.instances,item)
    else
        local x=output_local(event,context)
        local pill=pill_at_pointer(doc,event,context)
        if pill and (pill==item or contains_identity(item,pill.iid) or item.widget=="pill") then pill=nil end
        if type(event.slot)=="table" and type(event.slot.region)=="string" and type(event.slot.start)=="number" then
            item.host="top_bar"
            item.slot={region=event.slot.region,start=event.slot.start-.5,span=1}
            table.insert(doc.hosts.top_bar.instances,item)
        elseif pill then
            item.host=pill.iid
            pill.children=pill.children or {}
            insert_ordered(pill.children,item,x,"centre",context,false)
        else
            local width=output_dimensions(doc,event,context)
            local region=x<width/3 and "left" or x>width*2/3 and "right" or "centre"
            item.host="top_bar"
            insert_ordered(doc.hosts.top_bar.instances,item,x,region,context,true)
        end
    end
    config.enabled=true; config.style=style_name
    state.widget=id; state.settings=clone(item.settings or source.settings or state.settings or {})
    for _,other in group_items(doc,group,id) do other.settings=clone(state.settings) end
    if not normalise(doc,req) then return false end
    if form=="desktop" then config.placement={host="canvas",slot=clone(item.slot)} end
    commit_drop(s,doc,previous,commit)
    return true,doc,item
end
local function same_slot(a,b)
    if a.host~=b.host then return false end
    if a.host=="canvas" then
        return a.slot.x==b.slot.x and a.slot.y==b.slot.y and a.slot.w==b.slot.w and a.slot.h==b.slot.h
    end
    return a.slot.region==b.slot.region and a.slot.start==b.slot.start
end
-- The preview keeps the dimmed source in place and shows the target as a placeholder.
local function transient_layout(base,candidate,moved,source,epoch,req)
    if type(moved)~="table" then return candidate end
    local source_location=source.iid and locate_document_item(base,source.iid)
    if source_location and same_slot(source_location.item,moved) and source_location.item.host==moved.host then return candidate end
    local preview=clone(candidate)
    local target_location=locate_document_item(preview,moved.iid)
    if not target_location then return candidate end
    local placeholder=target_location.item
    local stem="drag-preview:"..tostring(epoch)..":"..tostring(moved.iid)
    placeholder.iid=string.sub(stem,1,128)
    local function taken(iid)
        local hit=false
        each_list(preview,function(other) if other~=placeholder and other.iid==iid then hit=true end end)
        return hit
    end
    local suffix=0
    while taken(placeholder.iid) do
        suffix+=1
        placeholder.iid=string.sub(stem,1,112)..":"..tostring(suffix)
    end
    placeholder.drag_placeholder=true
    if source_location and not locate_document_item(preview,source.iid) then
        local list=document_location(preview,source_location)
        if list then
            local original=clone(source_location.item)
            if original.slot.region then
                -- Re-seat the source between its original neighbours.
                local base_list=document_location(base,source_location) or {}
                local seated
                for index=source_location.index-1,1,-1 do
                    local neighbour=base_list[index]
                    if neighbour.slot.region~=original.slot.region then break end
                    seated=locate_document_item(preview,neighbour.iid)
                    if seated then break end
                end
                original.slot.start=seated and seated.item.slot.start+.5 or -.5
            end
            table.insert(list,original)
        end
    end
    if not normalise(preview,req) then return clone(base) end
    return preview
end
local function capture_drag_base(s,event,source,context)
    local original=document(s,context)
    if type(original)~="table" or type(original.hosts)~="table" or type(original.hosts.top_bar)~="table" then return nil end
    local base=clone(original)
    if not normalise(base,{catalogue=context.catalogue}) then return nil end
    s["widgets.drag.original"]={clone(original)}
    s["widgets.drag.base"]={clone(base)}
    s["widgets.drag.revision"]=tonumber(s["widgets.rev"]) or 0
    return base
end
local function op(s,args)
    local req=request(args)
    if not req then s["widgets.error"]="missing request"; return end
    local kind=req.op
    if kind=="select" then s["widgets.selected"]=req.widget or req.iid or ""; s["widgets.field"]=""; s["widgets.select"]=""; return end
    if kind=="back" then s["widgets.selected"]=""; s["widgets.field"]=""; s["widgets.select"]=""; return end
    if kind=="category" then s["widgets.category"]=req.value or "all"; s["widgets.selected"]=""; return end
    if kind=="form" then
        if req.value=="bar" or req.value=="desktop" then s["widgets.form"]=req.value end
        return
    end
    local doc=document(s,req)
    if type(doc)~="table" then s["widgets.error"]="missing layout document"; return end
    if kind=="field" then s["widgets.selected"]=req.widget or req.iid or ""; s["widgets.select"]=req.key or ""; s["widgets.field"]=tostring(req.value or ""); return end
    if kind=="menu" then
        local key="choice:"..tostring(req.key or "")
        s["widgets.select"]=s["widgets.select"]==key and "" or key
        return
    end
    if kind=="select" then s["widgets.selected"]=req.widget or req.iid or ""; s["widgets.select"]=""; return end
    local previous=clone(doc)
    if kind=="default-trigger" then
        if req.value~="click" and req.value~="hover" then s["widgets.error"]="invalid dropdown trigger"; return end
        doc.dropdown_trigger=req.value
        history(s,previous); finish(s,doc); return
    end
    if kind=="undo" then
        local undo=clone(s["widgets.undo"] or {})
        local previous=table.remove(undo)
        if previous then
            s["widgets.undo"]=undo
            s["widgets.draft"]={clone(previous.document)}
            s["widgets.error"]=""
        end
        return
    end
    if kind=="reset" then
        local initial=req.initial or req.document
        if type(initial)~="table" then s["widgets.error"]="missing default layout"; return end
        history(s,previous); finish(s,clone(initial)); return
    end
    local id=req.widget
    if type(id)~="string" or id=="" then s["widgets.error"]="invalid widget"; return end
    local package=catalogue(req,id)
    if not package then s["widgets.error"]="unknown widget"; return end
    local group,state=group_for(doc,id,req.group,req)
    local form=req.form or s["widgets.form"] or "bar"
    if form~="bar" and form~="desktop" then s["widgets.error"]="unsupported form"; return end
    local descriptor=package.forms and package.forms[form]
    if not descriptor then s["widgets.error"]="widget has no "..form.." form"; return end
    local form_state=state.forms[form] or {enabled=false,style=descriptor.default_style,placement={}}
    state.forms[form]=form_state
    local style_name=req.style or form_state.style
    if not style_name or style_name=="" then style_name=descriptor.legacy_style_setting and (state.settings or {})[descriptor.legacy_style_setting] end
    if not descriptor.styles or not descriptor.styles[style_name] then style_name=descriptor.default_style end
    local style=descriptor.styles and descriptor.styles[style_name]
    if not style then s["widgets.error"]="invalid style"; return end
    if kind=="setting" then
        if type(req.key)~="string" or req.key=="" then s["widgets.error"]="invalid setting key"; return end
        local value=req.from_field and s["widgets.field"] or req.value
        if value==nil then s["widgets.error"]="missing setting value"; return end
        update_setting(doc,group,state,req.key,value)
        history(s,previous)
        s["widgets.field"]=""; s["widgets.select"]=""
        finish(s,doc); return
    elseif kind=="style" then
        style_name=req.style
        style=descriptor.styles and descriptor.styles[style_name]
        if not style then s["widgets.error"]="invalid style"; return end
        form_state.style=style_name
        for _,item in group_items(doc,group,id) do
            if (item.host=="canvas" and "desktop" or "bar")==form then
                item.style=style_name
                if item.host=="canvas" then item.slot.w=math.min(style.preferred_span,doc.hosts.canvas.width-item.slot.x) end
            end
        end
    elseif kind=="remove" then
        -- Keyboard and edit-mode counterpart of dragging a widget off the bar or desktop.
        local target
        for _,item in group_items(doc,group,id) do
            if (item.host=="canvas")==(form=="desktop") then target=locate_document_item(doc,item.iid); break end
        end
        if not target then return end
        remove_instance(doc,state,target)
    else
        s["widgets.error"]="unknown widget operation"; return
    end
    if not normalise(doc,req) then s["widgets.error"]="invalid widget layout"; return end
    history(s,previous)
    finish(s,doc)
end
local function drag(s,args)
    s["widgets.write"]={}
    local event=request(args)
    if not event then return end
    local phase=event.phase
    if phase=="entry-seed" then
        local source=drag_record(event.source)
        local document=type(event.document)=="table" and clone(event.document) or current_canonical(s,event)
        local location=type(event.target_iid)=="string" and locate_document_item(document,event.target_iid) or nil
        if source and source.app==true and source.kind=="entry" and event.private_remote==true
            and type(document)=="table" and location and location.item.widget==source.widget
            and (location.item.group or location.item.iid)==source.group then
            location.item.drag_placeholder=true
            local epoch=(tonumber(s["widgets.drag.epoch"]) or 0)+1
            if epoch>=4503599627370496 then epoch=1 end
            s["widgets.drag.epoch"]=epoch
            s["widgets.drag.active"]=true
            s["widgets.drag.source"]={clone(source)}
            s["widgets.drag.destination"]=event.target_form
            s["widgets.drag.phase"]="dragging"
            s["widgets.drag.layout"]={document}
            s["widgets.drag.x"]=tonumber(event.x) or 0
            s["widgets.drag.y"]=tonumber(event.y) or 0
            s["widgets.drag.start_x"]=tonumber(event.x) or 0
            s["widgets.drag.start_y"]=tonumber(event.y) or 0
            s["widgets.drag.width"]=tonumber(event.width) or 0
            s["widgets.drag.height"]=tonumber(event.height) or 0
            s["widgets.drag.grab_x"]=tonumber(event.grab_x) or 0
            s["widgets.drag.grab_y"]=tonumber(event.grab_y) or 0
            s["widgets.drag.source_origin_x"]=tonumber(event.source_origin_x) or 0
            s["widgets.drag.source_origin_y"]=tonumber(event.source_origin_y) or 0
            s["widgets.drag.source_origin_valid"]=true
            s["widgets.drag.app_origin_x"]=tonumber(event.app_origin_x) or 0
            s["widgets.drag.app_origin_y"]=tonumber(event.app_origin_y) or 0
            s["widgets.drag.app_origin_valid"]=true
            s["widgets.drag.app_width"]=tonumber(event.app_width) or 0
            s["widgets.drag.app_height"]=tonumber(event.app_height) or 0
            s["widgets.drag.app_output_id"]=event.app_output_id or ""
            s["widgets.drag.target_origin_x"]=tonumber(event.target_origin_x) or 0
            s["widgets.drag.target_origin_y"]=tonumber(event.target_origin_y) or 0
            s["widgets.drag.target_output_id"]=event.target_output_id or ""
            s["widgets.drag.snap_x"]=tonumber(event.snap_x) or 0
            s["widgets.drag.snap_y"]=tonumber(event.snap_y) or 0
            s["widgets.drag.entry.target_width"]=tonumber(event.target_width) or 0
            s["widgets.drag.entry.target_height"]=tonumber(event.target_height) or 0
        end
        return
    elseif phase=="entry-flight" then
        local source=drag_record(s["widgets.drag.source"])
        if s["widgets.drag.active"]==true and s["widgets.drag.phase"]=="dragging"
            and source and source.kind=="entry"
            and tonumber(event.entry_epoch)==tonumber(s["widgets.drag.epoch"]) then
            s["widgets.drag.phase"]="settle"
        end
        return
    end
    if phase=="move" then
        local source=drag_record(event.source)
        if type(source)~="table" then return end
        local start=s["widgets.drag.phase"]~="dragging"
        if start then
            s["widgets.drag.epoch"]=(s["widgets.drag.epoch"] or 0)+1
            s["widgets.drag.source"]={clone(source)}
            s["widgets.drag.start_x"]=(tonumber(event.start_x) or tonumber(event.x) or 0)+(tonumber(event.source_origin_x) or 0)-(tonumber(event.target_origin_x) or 0)
            s["widgets.drag.start_y"]=(tonumber(event.start_y) or tonumber(event.y) or 0)+(tonumber(event.source_origin_y) or 0)-(tonumber(event.target_origin_y) or 0)
            s["widgets.drag.width"]=tonumber(event.width) or 0
            s["widgets.drag.height"]=tonumber(event.height) or 0
            s["widgets.drag.grab_x"]=tonumber(event.grab_x) or 0
            s["widgets.drag.grab_y"]=tonumber(event.grab_y) or 0
            local ox=tonumber(event.source_origin_x) or 0
            local oy=tonumber(event.source_origin_y) or 0
            s["widgets.drag.source_origin_x"]=ox
            s["widgets.drag.source_origin_y"]=oy
            s["widgets.drag.source_origin_valid"]=event.source_origin_valid==true
            if source.app==true then
                s["widgets.drag.app_origin_x"]=ox
                s["widgets.drag.app_origin_y"]=oy
                s["widgets.drag.app_origin_valid"]=event.source_origin_valid==true
                s["widgets.drag.app_width"]=tonumber(event.app_width) or 0
                s["widgets.drag.app_height"]=tonumber(event.app_height) or 0
                s["widgets.drag.app_output_id"]=event.app_output_id or ""
            end
        end
        local ox=tonumber(s["widgets.drag.source_origin_x"]) or 0
        local oy=tonumber(s["widgets.drag.source_origin_y"]) or 0
        local target_x=tonumber(event.target_origin_x) or 0
        local target_y=tonumber(event.target_origin_y) or 0
        local base=(s["widgets.drag.base"] or {})[1]
        if start or not base then
            base=capture_drag_base(s,event,source,type(event.context)=="table" and event.context or event)
        end
        s["widgets.drag.active"]=true
        s["widgets.drag.x"]=(tonumber(event.global_x) or ((tonumber(event.x) or 0)+ox))-target_x
        s["widgets.drag.y"]=(tonumber(event.global_y) or ((tonumber(event.y) or 0)+oy))-target_y
        s["widgets.drag.destination"]=event.destination or ""
        s["widgets.drag.phase"]="dragging"
        s["widgets.drag.target_origin_x"]=target_x
        s["widgets.drag.target_origin_y"]=target_y
        s["widgets.drag.target_output_id"]=event.target_output_id or event.app_output_id or ""
        if base then
            local context=type(event.context)=="table" and event.context or event
            local accepted,candidate,moved=drag_drop(s,event,context,base,false)
            s["widgets.drag.layout"]={accepted and (source.kind=="resize" and candidate or transient_layout(base,candidate,moved,source,s["widgets.drag.epoch"],{catalogue=context.catalogue})) or clone(base)}
        else
            s["widgets.drag.layout"]={}
        end
        return
    end
    if phase=="drop" then
        local source=drag_record(event.source)
        local accepted=false
        local context=type(event.context)=="table" and event.context or event
        local base=(s["widgets.drag.base"] or {})[1]
        local original=(s["widgets.drag.original"] or {})[1]
        if type(source)=="table" and not base then base=capture_drag_base(s,event,source,context); original=(s["widgets.drag.original"] or {})[1] end
        local candidate,moved
        local stale=base~=nil and drag_revision_stale(s)
        if not stale and type(source)=="table" and (source.app~=true or event.private_remote==true) and base then
            accepted,candidate,moved=drag_drop(s,event,context,base,true,original)
            if accepted then s["widgets.drag.layout"]={clone(candidate)} end
        end
        if accepted then
            s["widgets.drag.active"]=true
            s["widgets.drag.phase"]="settle"
            local live=moved
            if live and live.host=="canvas" then
                s["widgets.drag.snap_x"]=tonumber(live.slot.x) or 0
                s["widgets.drag.snap_y"]=tonumber(live.slot.y) or 0
            else
                -- Bar drops settle where the pointer released them; the bar reflows under it.
                s["widgets.drag.snap_x"]=(tonumber(s["widgets.drag.x"]) or 0)-(tonumber(s["widgets.drag.grab_x"]) or 0)
                s["widgets.drag.snap_y"]=0
            end
            if live then
                local settled=clone(source)
                settled.iid=live.iid; settled.group=live.group or source.group or live.iid; settled.host=live.host
                settled.form=live.host=="canvas" and "desktop" or "bar"
                settled.style=live.style; settled.settings=clone(live.settings or source.settings or {})
                s["widgets.drag.source"]={settled}
                s["widgets.drag.destination"]=settled.form
            end
        else
            if stale then
                s["widgets.draft"]={}
                s["widgets.error"]="Widget layout changed during drag; drag again."
                s["widgets.message"]=""
            end
            if source and source.app==true and event.source_origin_valid~=true then
                s["widgets.error"]="Float this window to drag a preview onto the desktop."
            end
            s["widgets.drag.active"]=true
            s["widgets.drag.phase"]="return"
            s["widgets.drag.destination"]=return_form(source or drag_record(s["widgets.drag.source"]))
            s["widgets.drag.snap_x"]=(tonumber(s["widgets.drag.start_x"]) or 0)-(tonumber(s["widgets.drag.grab_x"]) or 0)
            s["widgets.drag.snap_y"]=(tonumber(s["widgets.drag.start_y"]) or 0)-(tonumber(s["widgets.drag.grab_y"]) or 0)
            s["widgets.drag.layout"]={stale and current_canonical(s,context) or (base and clone(base) or current_canonical(s,context))}
        end
    end
    if phase=="cancel" then
        local base=(s["widgets.drag.base"] or {})[1]
        local stale=base~=nil and drag_revision_stale(s)
        local context=type(event.context)=="table" and event.context or event
        if stale then
            s["widgets.draft"]={}
            s["widgets.error"]="Widget layout changed during drag; drag again."
            s["widgets.message"]=""
        end
        s["widgets.drag.active"]=true
        s["widgets.drag.phase"]="return"
        s["widgets.drag.destination"]=return_form(drag_record(event.source) or drag_record(s["widgets.drag.source"]))
        s["widgets.drag.snap_x"]=(tonumber(s["widgets.drag.start_x"]) or 0)-(tonumber(s["widgets.drag.grab_x"]) or 0)
        s["widgets.drag.snap_y"]=(tonumber(s["widgets.drag.start_y"]) or 0)-(tonumber(s["widgets.drag.grab_y"]) or 0)
        s["widgets.drag.layout"]={stale and current_canonical(s,context) or (base and clone(base) or current_canonical(s,context))}
    end
    if phase=="retire" and tonumber(event.epoch)==tonumber(s["widgets.drag.epoch"]) then drag_clear(s) end
end

return {
    stores={
        ["window.floating"]=true,
        ["window.width"]=1100,["window.height"]=800,["window.min_width"]=420,["window.min_height"]=360,
        ["window.width_fraction"]=0.7,["window.height_fraction"]=0.7,
        ["widgets.layout"]={},["widgets.rev"]=0,["widgets.category"]="all",["widgets.search"]="",
        ["widgets.selected"]="",["widgets.group"]="",["widgets.form"]="bar",["widgets.advanced.open"]=false,["widgets.scroll"]=0,["widgets.nav.scroll"]=0,
        ["widgets.screen_width"]=1920,["widgets.screen_height"]=1080,
        ["widgets.screen_origin_x"]=0,["widgets.screen_origin_y"]=0,["widgets.screen_id"]="",["widgets.outputs"]={},
        ["widgets.wallpaper.path"]="",
        ["launcher.apps"]={},["dashboard.host"]={},["bluetooth.devices"]={},["bluetooth.power_checked"]=false,
        ["network.rows"]={},["network.wifi_enabled"]=false,["notifications.rows"]={},
        ["status.updates_checked"]="Never checked",["status.updates_can_open"]=false,["status.updates_can_refresh"]=false,
        ["session.enabled"]=false,["session.busy"]=false,["session.confirm"]="",["session.confirm_action"]="",["session.message"]="",
        ["status.hovered"]="",["status.peek.visible"]=false,["status.peek.kind"]="",["status.panel.kind"]="",
        ["widgets.drag.active"]=false,["widgets.drag.source"]={},["widgets.drag.destination"]="",
        ["widgets.drag.x"]=0,["widgets.drag.y"]=0,["widgets.drag.start_x"]=0,["widgets.drag.start_y"]=0,
        ["widgets.drag.phase"]="",["widgets.drag.epoch"]=0,["widgets.drag.snap_x"]=0,["widgets.drag.snap_y"]=0,
        ["widgets.drag.base"]={},["widgets.drag.original"]={},["widgets.drag.layout"]={},
        ["widgets.drag.revision"]=0,
        ["widgets.drag.width"]=0,["widgets.drag.height"]=0,["widgets.drag.grab_x"]=0,["widgets.drag.grab_y"]=0,
        ["widgets.drag.target_origin_x"]=0,["widgets.drag.target_origin_y"]=0,
        ["widgets.drag.target_output_id"]="",
        ["widgets.drag.source_origin_x"]=0,["widgets.drag.source_origin_y"]=0,["widgets.drag.source_origin_valid"]=false,
        ["widgets.drag.app_origin_x"]=0,["widgets.drag.app_origin_y"]=0,["widgets.drag.app_origin_valid"]=false,
        ["widgets.drag.app_width"]=0,["widgets.drag.app_height"]=0,
        ["widgets.drag.entry.target_width"]=0,["widgets.drag.entry.target_height"]=0,
        ["widgets.drag.app_output_id"]="",["widgets.app.open"]=false,
        ["widgets.message"]="",["widgets.error"]="",["widgets.busy"]=false,["widgets.draft"]={},["widgets.undo"]={},
        ["widgets.history.revision"]=0,
        ["widgets.write"]={},["widgets.initial"]={},["widgets.catalogue"]={},["widgets.field"]="",["widgets.select"]="",
        ["widgets.editing"]=true,["panel.open"]=true,
        ["widgets.preview"]={{["bead.desktop.audio"]="volume_up",["bead.desktop.audio.label"]="42%",
            ["bead.desktop.network"]="wifi",["bead.desktop.bluetooth"]="bluetooth",
            ["bead.desktop.battery"]="battery_full",["bead.desktop.battery.value"]="83%",["bar.title"]="Notes",["bar.clock.text"]="12:34",
            ["bead.desktop.clock.date"]="2026-10-25",["bead.desktop.clock.seconds"]={fallback="12:34:56"}}},
        ["settings.page"]="wallpaper",["settings.search"]="",["settings.nav"]={},["settings.items"]={},
        ["settings.title"]="",["settings.description"]="",["settings.message"]="",["settings.notice"]="",
        ["settings.noticed"]=false,["settings.writable"]=false,["settings.readonly"]=true,
        ["settings.searching"]=false,["settings.empty"]=true,
        ["bar.height"]=50,["bar.launcher"]=true,["bar.popouts"]=true,["bar.reconnecting"]=false,
        ["bar.workspace.enabled"]=true,["bar.tray.enabled"]=true,["bar.window.focus.enabled"]=true,
        ["bar.workspace.origins"]={},["bar.dashboard.enabled"]=true,["bar.tray.visible"]=false,
        ["bar.workspace.count"]=6,["bar.workspace.icons"]={},["bar.workspace.hints"]=true,
        ["bar.clock.twelve"]=false,["bar.updates.count"]=3,["bar.updates.enabled"]=true,
        ["bar.notifications.count"]=2,["bead.edge.dwell"]=0,["bead.edge.fade"]=0,
        ["bead.windows"]={},["bead.workspaces"]={},["bead.focused_title"]="",
        ["bead.desktop.audio.percent"]=0,["bead.desktop.audio.device"]="Output unavailable",
        ["bead.desktop.audio.label"]="Unavailable",["bead.desktop.titlevisible"]=true,
        ["bead.desktop.clock"]="",["bead.desktop.clock.seconds"]="",["bead.desktop.clock.date"]="",["bead.desktop.wallpaper.luma"]={},
        ["bead.desktop.audio"]="volume_mute",["bead.desktop.audio.available"]=false,
        ["bead.desktop.battery"]="battery_unknown",["bead.desktop.battery.value"]="",
        ["bead.desktop.battery.value.width"]=3,["bead.desktop.network"]="wifi_off",
        ["bead.desktop.bluetooth"]="bluetooth_disabled",["bead.desktop.tray"]={},
        ["bead.desktop.menu"]={},["bead.desktop.tray.tooltip"]="",
        ["bead.desktop.tray.tooltip.visible"]=false,
    },
    actions={
        ["widgets.drag"]=drag,
        ["widgets.history.parent"]=parent_history,
        ["widgets.navigate"]=function(s,args)
            local req=request(args)
            if not req then return end
            if req.op=="select" then s["widgets.selected"]=req.value or ""
            elseif req.op=="back" then s["widgets.selected"]=""
            elseif req.op=="category" then s["widgets.category"]=req.value or "all"; s["widgets.selected"]=""
            elseif req.op=="form" then
                if req.value=="bar" or req.value=="desktop" then s["widgets.form"]=req.value end
            end
        end,
        ["widgets.advanced.toggle"]=function(s) s["widgets.advanced.open"]=not s["widgets.advanced.open"] end,
        ["widgets.edit"]=op,
    },
    computed={
        ["bar.clock.text"]=function(s)
            local clock=s["bead.desktop.clock"]
            local h,m=string.match(clock,"^(%d+):(%d%d)$")
            if not s["bar.clock.twelve"] or not h then return clock end
            local hour=tonumber(h)
            return string.format("%d:%s %s",(hour+11)%12+1,m,hour<12 and "AM" or "PM")
        end,
        ["bar.updates.text"]=function(s) return tostring(s["bar.updates.count"]) end,
        ["bar.clock.width"]=function(s) return s["bar.clock.twelve"] and 66 or 44 end,
        ["bar.title.cue.visible"]=function(s) return s["bead.edge.fade"]>0 end,
        ["bar.title.cue.fade"]=function(s) return s["bead.edge.fade"] end,
        ["bar.title.cue.halo"]=function(s) return .025*s["bead.edge.dwell"]*s["bead.edge.fade"] end,
        ["bar.title.cue.body"]=function(s) return (.03+.09*s["bead.edge.dwell"])*s["bead.edge.fade"] end,
        ["bar.title.cue.border"]=function(s) return (.06+.18*s["bead.edge.dwell"])*s["bead.edge.fade"] end,
        ["bar.notifications.text"]=function(s) return tostring(math.min(999,s["bar.notifications.count"])) end,
        ["bar.notifications.tail"]=function(s) return 7+7.5*#tostring(math.min(999,s["bar.notifications.count"])) end,
        ["bar.notifications.detail.width"]=function(s) return 1+8*#s["bar.notifications.text"] end,
        ["bar.battery.detail.width"]=function(s) return 1+8*#s["bead.desktop.battery.value"] end,
        ["bar.title"]=function(s)
            local title=s["bead.focused_title"]
            return title~="" and title or tr("bead.bar.desktop-title")
        end,
        ["widgets.wallpaper"]=function(s)
            for _,row in s["settings.items"] or {} do if row.kind=="wallpaper-scene" then return row.preview or "" end end
            return ""
        end,
    },
}
