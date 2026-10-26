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
local function sync_bar_placement(doc,item,old_host,old_slot)
    local widgets=doc.widgets
    if type(widgets)~="table" then return end
    local state=widgets[item.group or item.iid]
    if type(state)~="table" or type(state.forms)~="table" then return end
    local bar=state.forms.bar
    if type(bar)~="table" or type(bar.placement)~="table" then return end
    local placement=bar.placement
    local slot=placement.slot
    if placement.host==old_host and type(slot)=="table"
        and slot.region==old_slot.region
        and tonumber(slot.start)==tonumber(old_slot.start)
        and tonumber(slot.span)==tonumber(old_slot.span) then
        slot.region=item.slot.region
        slot.start=item.slot.start
        slot.span=item.slot.span
    end
end
local function pack_bar_region(doc,region_name)
    local host=doc.hosts.top_bar
    local bounds=host.regions[region_name]
    if not bounds then return false end
    local rows={}
    for _,item in host.instances do if item.slot and item.slot.region==region_name then table.insert(rows,item) end end
    table.sort(rows,function(a,b) return a.slot.start<b.slot.start end)
    local start=0
    for _,item in rows do
        item.slot.start=start
        start+=item.slot.span
    end
    return start<=bounds.span
end
local function reflow_bar_region(doc,region_name,target_item,target_start)
    local host=doc.hosts.top_bar
    local bounds=host.regions[region_name]
    if not bounds then return false end
    -- A resize passes the existing root by reference; only reject a conflicting IID,
    -- and append only when this is a genuinely new root.
    local target_present=false
    for _,item in host.instances do
        if item.iid==target_item.iid then
            if item~=target_item or target_present then return false end
            target_present=true
        end
    end
    target_start=math.max(0,math.min(bounds.span-target_item.slot.span,target_start))
    local rows={}
    for _,item in host.instances do
        if item~=target_item and item.slot and item.slot.region==region_name then table.insert(rows,item) end
    end
    table.sort(rows,function(a,b) return a.slot.start<b.slot.start end)
    local index=1
    for i,item in rows do
        if target_start < item.slot.start+item.slot.span/2 then index=i; break end
        index=i+1
    end
    local before,after={},{}
    for i,item in rows do table.insert(i<index and before or after,item) end
    local cursor=target_start+target_item.slot.span
    for _,item in after do
        item.slot.start=math.max(item.slot.start,cursor)
        cursor=item.slot.start+item.slot.span
    end
    if cursor>bounds.span then return false end
    cursor=target_start
    for i=#before,1,-1 do
        local item=before[i]
        item.slot.start=math.min(item.slot.start,cursor-item.slot.span)
        cursor=item.slot.start
    end
    if cursor<0 then return false end
    target_item.slot.region=region_name
    target_item.slot.start=target_start
    if not target_present then table.insert(host.instances,target_item) end
    return true
end
local function reflow_preview_region(doc,region_name,source_iid)
    local host=doc.hosts.top_bar
    local bounds=host.regions[region_name]
    if not bounds then return false end
    local rows,occupied={},{}
    for _,item in host.instances do
        if item.slot and item.slot.region==region_name then
            table.insert(rows,item)
        end
    end
    table.sort(rows,function(a,b) return a.slot.start<b.slot.start end)
    local source_item,placeholder
    for _,item in rows do
        if item.iid==source_iid then source_item=item end
        if item.drag_placeholder==true then placeholder=item end
    end
    if source_item and placeholder then
        local source_start,source_end=source_item.slot.start,source_item.slot.start+source_item.slot.span
        local placeholder_start,placeholder_end=placeholder.slot.start,placeholder.slot.start+placeholder.slot.span
        if source_start<placeholder_end and placeholder_start<source_end then
            local left=source_start-placeholder.slot.span
            local right=source_end
            if left>=0 and (right+placeholder.slot.span>bounds.span or math.abs(placeholder_start-left)<=math.abs(right-placeholder_start)) then
                placeholder.slot.start=left
            else
                placeholder.slot.start=right
            end
        end
    end
    occupied={}
    for _,item in rows do
        if item.iid==source_iid or item.drag_placeholder==true then
            for tile=item.slot.start,item.slot.start+item.slot.span-1 do
                if occupied[tile] then return false end
                occupied[tile]=true
            end
        end
    end
    for _,item in rows do
        if item.iid~=source_iid and item.drag_placeholder~=true then
            local start=item.slot.start
            local free=function(at)
                if at<0 or at+item.slot.span>bounds.span then return false end
                for tile=at,at+item.slot.span-1 do if occupied[tile] then return false end end
                return true
            end
            while not free(start) and start+item.slot.span<=bounds.span do start+=1 end
            if not free(start) then return false end
            item.slot.start=start
            for tile=start,start+item.slot.span-1 do occupied[tile]=true end
        end
    end
    return true
end
local function next_bar_slot(doc,region,span)
    local host=doc.hosts.top_bar
    local bounds=host.regions[region]
    if not bounds or span<1 or span>bounds.span then return nil end
    local occupied={}
    local function fill(items)
        for _,item in items or {} do
            if item.slot and item.slot.region==region then
                for n=item.slot.start,item.slot.start+item.slot.span-1 do occupied[n]=true end
            end
        end
    end
    fill(host.instances)
    for start=0,bounds.span-span do
        local free=true
        for n=start,start+span-1 do if occupied[n] then free=false; break end end
        if free then return start end
    end
end
local function bar_slot_free(doc,region,start,span)
    local host=doc.hosts.top_bar
    local bounds=host.regions[region]
    if not bounds or start<0 or start+span>bounds.span then return false end
    local occupied={}
    for _,item in host.instances do
        if item.slot and item.slot.region==region then
            for n=item.slot.start,item.slot.start+item.slot.span-1 do occupied[n]=true end
        end
    end
    for n=start,start+span-1 do if occupied[n] then return false end end
    return true
end
local function insert_bar(doc,item,placement,style)
    local host=doc.hosts.top_bar
    local region=placement and placement.slot and placement.slot.region or "left"
    if not host.regions[region] then region="left" end
    local span=placement and placement.slot and placement.slot.span or (style and style.preferred_span) or 1
    span=math.max(1,math.floor(tonumber(span) or 1))
    if style and (span<style.min_span or span>style.max_span) then span=style.preferred_span end
    local start=placement and placement.slot and placement.slot.start
    if type(start)~="number" or not bar_slot_free(doc,region,start,span) then start=next_bar_slot(doc,region,span) end
    if start==nil then return false end
    item.host="top_bar"; item.slot={region=region,start=start,span=span}
    table.insert(host.instances,item)
    return true
end
local function add_desktop(doc,item,placement,style,req)
    local host=doc.hosts.canvas
    if not host then
        host={width=req.screen_width or 1920,height=req.screen_height or 1080,instances={}}
        doc.hosts.canvas=host
    end
    local slot=placement and placement.slot or {}
    local w=math.clamp(tonumber(slot.w) or style.preferred_span,style.min_span,style.max_span)
    local h=tonumber(slot.h) or style.preferred_height or 96
    if w>host.width or h<=0 or h>host.height then return false end
    item.host="canvas"; item.slot={x=math.clamp(tonumber(slot.x) or 32,0,host.width-w),
        y=math.clamp(tonumber(slot.y) or 32,0,host.height-h),w=w,h=h}
    table.insert(host.instances,item)
    return true
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
local function promote_children(doc,pill)
    local items=pill.children or {}
    for _,child in items do
        child.host="top_bar"
        local slot={region=child.slot and child.slot.region or "left",span=child.slot and child.slot.span or 1}
        child.slot=slot
        if not insert_bar(doc,child,{slot=slot}) then return false end
    end
    pill.children={}
    return true
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
local function top_bar_root(doc,iid)
    for _,item in doc.hosts.top_bar.instances or {} do
        if contains_identity(item,iid) then return item end
    end
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
local function pill_limits(context,pill)
    local package=catalogue(context,"pill")
    local form=package and package.forms and package.forms.bar
    local style=form and form.styles and form.styles[pill.style or form.default_style]
    return style and style.min_span or 1,style and style.max_span or (context.tiles or 60)
end
local function resize_pill(doc,pill,delta,context)
    if delta==0 then return true end
    local minimum,maximum=pill_limits(context,pill)
    local span=pill.slot.span+delta
    if span<minimum or span>maximum then return false end
    local root=top_bar_root(doc,pill.iid)
    if not root or not root.slot then return false end
    root.slot.span=span
    return reflow_bar_region(doc,root.slot.region,root,root.slot.start)
end
local function reflow_pill_child_region(doc,pill,target,span)
    local rows={}
    for _,child in pill.children or {} do
        if child.slot and child.slot.region==target.slot.region then table.insert(rows,child) end
    end
    table.sort(rows,function(a,b) return a.slot.start<b.slot.start end)
    local found=false
    local cursor=0
    for _,child in rows do
        if child==target then
            child.slot.span=span
            cursor=child.slot.start+span
            found=true
        elseif found then
            if child.slot.start<cursor then child.slot.start=cursor end
            cursor=child.slot.start+child.slot.span
        end
    end
    local limit=doc.hosts.top_bar.tiles or 60
    return found and cursor<=limit
end
local function preferred_bar_span(current,style)
    current=math.max(1,math.floor(tonumber(current) or 1))
    local minimum=math.max(1,math.floor(tonumber(style.min_span) or 1))
    local maximum=math.max(minimum,math.floor(tonumber(style.max_span) or current))
    local preferred=math.floor(tonumber(style.preferred_span) or minimum)
    return math.max(current,minimum,math.min(maximum,preferred))
end
local function resize_bar(doc,item,style,context)
    local old_host=item.host
    local old_slot=table.clone(item.slot)
    local current=tonumber(item.slot and item.slot.span) or tonumber(style.min_span) or 1
    local span=preferred_bar_span(current,style)
    local delta=span-current
    if delta<=0 then return true end
    if item.host=="top_bar" then
        item.slot.span=span
        if not reflow_bar_region(doc,item.slot.region,item,item.slot.start) then return false end
    else
        local location=locate_document_item(doc,item.iid)
        local parent=location and location.parent and locate_document_item(doc,location.parent)
        if not parent or parent.item.widget~="pill" then return false end
        if not reflow_pill_child_region(doc,parent.item,item,span) then return false end
        if not resize_pill(doc,parent.item,delta,context) then return false end
    end
    sync_bar_placement(doc,item,old_host,old_slot)
    return true
end
local function grow_saved_bar_placement(form_state,style)
    local placement=form_state.placement
    local slot=type(placement)=="table" and placement.slot or nil
    if type(slot)~="table" then return end
    local current=tonumber(slot.span) or tonumber(style.min_span) or 1
    local span=preferred_bar_span(current,style)
    if span>current then slot.span=span end
end
local function child_slot(pill,span,event,context,doc,omit_iid)
    local x=output_local(event,context)
    local rect
    for _,candidate in context.geometry or {} do if candidate.iid==pill.iid then rect=candidate; break end end
    local region="centre"
    if rect and rect.w>0 then
        local fraction=(x-rect.x)/rect.w
        region=fraction<1/3 and "left" or fraction>2/3 and "right" or "centre"
    end
    local tiles=doc.hosts.top_bar.tiles or 60
    for _,candidate_region in {region,"centre","left","right"} do
        local occupied={}
        for _,child in pill.children or {} do
            if child.iid~=omit_iid and child.slot and child.slot.region==candidate_region then
                for tile=child.slot.start,child.slot.start+child.slot.span-1 do occupied[tile]=true end
            end
        end
        for start=0,tiles-span do
            local free=true
            for tile=start,start+span-1 do if occupied[tile] then free=false; break end end
            if free then return {region=candidate_region,start=start,span=span} end
        end
    end
end
local function tile_document(doc,req)
    local host=doc.hosts.top_bar
    if not host or host.mode~="content" then return true end
    local geometry,reservations={},{}
    for _,item in req.reservations or {} do reservations[item.iid]=item.width end
    for _,rect in req.geometry or {} do geometry[rect.iid]=rect end
    local width=tonumber(req.canvas_width) or tonumber(req.screen_width)
    if not width or width<=0 or not host.tiles or host.tiles<=0 then return false end
    local tile=width/host.tiles
    local function pack(items,bounds,parent_rect,tiles)
        for _,region in {"left","centre","right"} do
            local rows={}
            for _,item in items or {} do if item.slot.region==region then table.insert(rows,item) end end
            table.sort(rows,function(a,b)
                local ar,br=geometry[a.iid],geometry[b.iid]
                local ax=ar and ar.x or a.slot.start*tile
                local bx=br and br.x or b.slot.start*tile
                if parent_rect then ax-=parent_rect.x; bx-=parent_rect.x end
                if region=="right" then
                    ax=(parent_rect and parent_rect.w or width)-ax-(ar and ar.w or a.slot.span*tile)
                    bx=(parent_rect and parent_rect.w or width)-bx-(br and br.w or b.slot.span*tile)
                end
                return ax<bx
            end)
            local region_bounds=bounds[region]
            local start=region_bounds.start
            for _,item in rows do
                local old_host,old_slot=item.host,table.clone(item.slot)
                local rect=geometry[item.iid]
                local measured=rect and rect.w or (item.slot.span*tile)
                if parent_rect and rect then measured=rect.w end
                measured=math.max(measured or 0,reservations[item.iid] or 0)
                local span=math.max(1,math.ceil(measured/tile))
                local position
                if rect then
                    local x=rect.x-(parent_rect and parent_rect.x or 0)
                    if region=="right" then x=(parent_rect and parent_rect.w or width)-x-rect.w end
                    if region=="centre" then x=x+rect.w/2-span*tile/2 end
                    position=math.floor(x/tile)
                else position=item.slot.start end
                start=math.max(start,region_bounds.start+position)
                if start+span>region_bounds.start+region_bounds.span then return false end
                item.slot.start=start-region_bounds.start
                item.slot.span=span
                sync_bar_placement(doc,item,old_host,old_slot)
                start+=span
                if item.children and #item.children>0 then
                    local child_width=math.max(rect and rect.w or 0,span*tile)
                    local child_tiles=math.max(1,math.floor(child_width/tile))
                    local nested={left={start=0,span=child_tiles},centre={start=0,span=child_tiles},right={start=0,span=child_tiles}}
                    if not pack(item.children,nested,rect,{tiles=child_tiles}) then return false end
                end
            end
        end
        return true
    end
    local bounds={}
    for name,region in host.regions do bounds[name]={start=region.start,span=region.span} end
    if not pack(host.instances,bounds,nil,host) then return false end
    host.mode="tiles"
    doc.preset="Custom"
    return true
end
local function bar_drop_placement(doc,event,context,span)
    if type(event.slot)=="table" and type(event.slot.region)=="string" and type(event.slot.start)=="number" then
        local slot=clone(event.slot)
        slot.span=span
        return {host="top_bar",slot=slot}
    end
    local host=doc.hosts.top_bar
    local width=tonumber(event.screen_width) or tonumber(context.screen_width) or tonumber(context.canvas_width) or 1920
    local x=output_local(event,context)
    local region=x<width/3 and "left" or x>width*2/3 and "right" or "centre"
    local bounds=host.regions[region]
    local tile=width/math.max(1,host.tiles)
    local position=region=="right" and width-x or x
    if region=="centre" then position-=span*tile/2 end
    local start=math.floor(position/tile)
    start=math.max(0,math.min(bounds.span-span,start))
    return {host="top_bar",slot={region=region,start=start,span=span}}
end
local function pill_overlapping_bar_slot(doc,placement)
    local slot=placement and placement.slot
    if type(slot)~="table" or type(slot.region)~="string" then return nil end
    local first=tonumber(slot.start) or 0
    local last=first+(tonumber(slot.span) or 1)
    for _,item in doc.hosts.top_bar.instances or {} do
        if item.widget=="pill" and item.slot and item.slot.region==slot.region then
            local item_first=tonumber(item.slot.start) or 0
            local item_last=item_first+(tonumber(item.slot.span) or 1)
            if first<item_last and item_first<last then return item end
        end
    end
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
local function insert_into_pill(doc,item,pill,event,context,style)
    if item.iid==pill.iid or contains_identity(item,pill.iid) then return false end
    local location=item.iid and locate_document_item(doc,item.iid)
    local old_parent=location and location.parent and locate_document_item(doc,location.parent)
    if old_parent and old_parent.item.widget~="pill" then return false end
    if location then
        local source_list=document_location(doc,location)
        if not source_list then return false end
        table.remove(source_list,location.index)
    end
    local span=location and location.item.slot and location.item.slot.span
        or (style.preferred_span or style.min_span or 1)
    span=math.floor(math.clamp(tonumber(span) or style.preferred_span or 1,style.min_span or 1,style.max_span or 60))
    if old_parent and old_parent.item.iid~=pill.iid then
        if not resize_pill(doc,old_parent.item,-span,context) then return false end
    elseif location and not location.parent and location.host=="top_bar" then
        if not pack_bar_region(doc,location.item.slot.region) then return false end
    end
    local slot=child_slot(pill,span,event,context,doc,item.iid)
    if not slot then return false end
    pill.children=pill.children or {}
    item.host=pill.iid
    item.slot=slot
    table.insert(pill.children,item)
    if not old_parent or old_parent.item.iid~=pill.iid then
        if not resize_pill(doc,pill,span,context) then return false end
    else
        local root=top_bar_root(doc,pill.iid)
        if not root or not reflow_bar_region(doc,root.slot.region,root,root.slot.start) then return false end
    end
    return true
end
local function drag_drop(s,event,context,base,commit,undo_document)
    local source=drag_record(event.source)
    if type(source)~="table" or type(source.widget)~="string" then return false end
    -- Canvas resize gestures use the same generic transfer payload but have their own
    -- transaction semantics; keep them out of the placement/morph path.
    if source.kind=="resize" then return drag_resize(s,event,context,base,commit,undo_document,source) end
    if source.app==true and event.private_remote~=true then return false end
    local destination=event.destination
    if destination~="bar" and destination~="desktop" then return false end
    if source.app==true and event.source_origin_valid~=true then return false end
    local id=source.widget
    local req={catalogue=context.catalogue,settings=source.settings,group=source.group}
    local doc=type(base)=="table" and clone(base) or document(s,context)
    if type(doc)~="table" then return false end
    local package=catalogue(req,id)
    local form=destination=="bar" and "bar" or "desktop"
    local descriptor=package and package.forms and package.forms[form]
    if not descriptor then return false end
    if (source.form=="bar" or form=="bar") and not tile_document(doc,context) then return false end
    local previous=undo_document or clone(doc)
    local group,state=group_for(doc,id,source.group or source.iid or id,req)
    local config=state.forms[form] or {enabled=false,style=descriptor.default_style,placement={}}
    state.forms[form]=config
    local style_name=config.style
    if type(style_name)~="string" or style_name=="" or not descriptor.styles[style_name] then
        local legacy=descriptor.legacy_style_setting and source.settings and source.settings[descriptor.legacy_style_setting]
        style_name=descriptor.styles[source.style] and source.style or (descriptor.styles[legacy] and legacy) or descriptor.default_style
    end
    if not descriptor.styles[style_name] then style_name=descriptor.default_style end
    local style=descriptor.styles[style_name]
    if not style then return false end
    local source_form=source.form or (source.host=="canvas" and "desktop" or "bar")
    local live
    for _,item in group_items(doc,group,id) do if item.iid==source.iid and source.iid then live=item; break end end
    local live_location=live and locate_document_item(doc,live.iid)
    local live_parent=live_location and live_location.parent and locate_document_item(doc,live_location.parent)
    local existing
    for _,item in group_items(doc,group,id) do if (item.host=="canvas")== (form=="desktop") then existing=item; break end end
    if live and source_form==form then existing=live end
    local item=existing or (live and clone(live)) or {
        iid=form=="desktop" and (group..":desktop") or group,widget=id,group=group,
        settings=clone(state.settings or source.settings or package.defaults or {}),
    }
    item.group=group; item.style=style_name
    item.settings=item.settings or clone(state.settings or source.settings or package.defaults or {})
    for key,value in source.settings or {} do item.settings[key]=clone(value) end
    local placement
    local desktop_context=context
    if form=="desktop" then
        local width,height=output_dimensions(doc,event,context)
        desktop_context=table.clone(context)
        desktop_context.screen_width=width; desktop_context.screen_height=height
        desktop_context.canvas_width=width; desktop_context.canvas_height=height
        if doc.hosts.canvas then
            doc.hosts.canvas.width=width
            doc.hosts.canvas.height=height
        end
        local x,y=output_local(event,context)
        local configured=config.placement and config.placement.slot or {}
        local w=tonumber(configured.w) or (existing and existing.slot.w) or style.preferred_span
        local h=tonumber(configured.h) or (existing and existing.slot.h) or style.preferred_height or 96
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
        placement={host="canvas",slot={x=magnet(tonumber(event.slot and event.slot.x) or x,width-w),
            y=magnet(tonumber(event.slot and event.slot.y) or y,height-h),w=w,h=h}}
    else
        local configured=config.placement and config.placement.slot and config.placement.slot.span
        local existing_span=existing and existing.slot and existing.slot.span
        local span=tonumber(configured) or tonumber(existing_span) or style.preferred_span or style.min_span
        span=math.floor(math.clamp(span,style.min_span or 1,style.max_span or 60))
        placement=bar_drop_placement(doc,event,context,span)
    end
    -- The drag anchor can sit just outside the pill while the destination slot
    -- overlaps it (large desktop forms make this common). Treat that occupied
    -- slot as a pill drop so the widget joins the existing pill instead of
    -- trying to wedge a new top-level item through its reservation.
    local target_pill=form=="bar" and (pill_at_pointer(doc,event,context)
        or pill_overlapping_bar_slot(doc,placement)) or nil
    if target_pill then
        local source_form=source.form or (source.host=="canvas" and "desktop" or "bar")
        if live and source_form~=form and not existing then
            local old=state.forms[source_form]
            if old then
            old.enabled=false
            if not old.placement or type(old.placement.slot)~="table" then
                old.placement={host=live.host,slot=clone(live.slot)}
            end
        end
        end
        if not insert_into_pill(doc,item,target_pill,event,context,style) then return false end
        config.enabled=true; config.style=style_name
        config.placement={host=target_pill.iid,slot=clone(item.slot)}
        state.widget=id; state.settings=clone(item.settings or source.settings or state.settings or {})
        for _,other in group_items(doc,group,id) do other.settings=clone(state.settings) end
        if commit~=false then
            history(s,previous); finish(s,doc)
            s["widgets.write"]={clone(doc)}
        end
        return true,doc,item
    end
    -- Existing second form stays enabled; moving a live source into an absent form morphs it.
    if live and source_form~=form and not existing then
        if live.widget=="pill" and live.children and #live.children>0 and source_form=="bar" then
            if not promote_children(doc,live) then return false end
            item.children={}
        end
        local old_region=live_location and not live_location.parent and live_location.host=="top_bar"
            and live.slot and live.slot.region or nil
        local list=live.host=="canvas" and doc.hosts.canvas.instances or doc.hosts.top_bar.instances
        local removed=remove_identity(list,live.iid)
        if not removed then
            for _,parent in doc.hosts.top_bar.instances do
                removed=remove_identity(parent.children or {},live.iid)
                if removed then break end
            end
        end
        if not removed then return false end
        if live_parent and live_parent.item.widget=="pill" then
            if not resize_pill(doc,live_parent.item,-(live.slot and live.slot.span or 1),context) then return false end
        end
        if old_region and not pack_bar_region(doc,old_region) then return false end
        local old=state.forms[source_form]
        if old then
            old.enabled=false
            if not old.placement or type(old.placement.slot)~="table" then
                old.placement={host=live.host,slot=clone(live.slot)}
            end
        end
    elseif existing then item=existing end
    if form=="desktop" then
        if existing then
            item.host="canvas"; item.slot={x=placement.slot.x,y=placement.slot.y,w=placement.slot.w,h=placement.slot.h}
        else
            if not add_desktop(doc,item,placement,style,desktop_context) then return false end
        end
    else
        local span=tonumber(placement.slot.span) or tonumber(item.slot and item.slot.span) or style.preferred_span
        span=math.max(style.min_span,math.min(style.max_span,math.floor(span)))
        local item_location=item.iid and locate_document_item(doc,item.iid)
        local item_parent=item_location and item_location.parent and locate_document_item(doc,item_location.parent)
        local old_region=item_location and not item_location.parent and item_location.host=="top_bar"
            and item.slot and item.slot.region or nil
        remove_identity(doc.hosts.top_bar.instances,item.iid)
        if item_parent and item_parent.item.widget=="pill" then
            if not resize_pill(doc,item_parent.item,-(item.slot and item.slot.span or 1),context) then return false end
        end
        if old_region and not pack_bar_region(doc,old_region) then return false end
        placement.slot.span=span
        item.host="top_bar"; item.slot=clone(placement.slot)
        table.insert(doc.hosts.top_bar.instances,item)
        if not reflow_bar_region(doc,item.slot.region,item,placement.slot.start) then return false end
    end
    if form=="bar" then placement.slot=clone(item.slot) end
    config.enabled=true; config.style=style_name; config.placement=placement
    state.widget=id; state.settings=clone(item.settings or source.settings or state.settings or {})
    for _,other in group_items(doc,group,id) do other.settings=clone(state.settings) end
    if commit~=false then
        history(s,previous); finish(s,doc)
        s["widgets.write"]={clone(doc)}
    end
    return true,doc,item
end
local function same_slot(a,b)
    if a.host~=b.host then return false end
    if a.host=="canvas" then
        return a.slot.x==b.slot.x and a.slot.y==b.slot.y and a.slot.w==b.slot.w and a.slot.h==b.slot.h
    end
    return a.slot.region==b.slot.region and a.slot.start==b.slot.start and a.slot.span==b.slot.span
end
local function transient_layout(base,candidate,moved,source,epoch)
    if type(moved)~="table" then return candidate end
    local source_location=source.iid and locate_document_item(base,source.iid)
    if source_location and same_slot(source_location.item,moved) then return candidate end
    local preview=clone(candidate)
    local target_location=locate_document_item(preview,moved.iid)
    if not target_location then return candidate end
    local target_list=document_location(preview,target_location)
    if not target_list then return candidate end
    table.remove(target_list,target_location.index)
    local placeholder=clone(moved)
    local stem="drag-preview:"..tostring(epoch)..":"..tostring(moved.iid)
    placeholder.iid=string.sub(stem,1,128)
    local suffix=0
    while locate_document_item(preview,placeholder.iid) do
        suffix+=1
        placeholder.iid=string.sub(stem,1,112)..":"..tostring(suffix)
    end
    placeholder.drag_placeholder=true
    table.insert(target_list,target_location.index,placeholder)
    if source_location and not locate_document_item(preview,source.iid) then
        local source_list=document_location(preview,source_location)
        if source_list then table.insert(source_list,math.min(source_location.index,#source_list+1),clone(source_location.item)) end
    end
    if source_location and source_location.parent then
        local original_parent=locate_document_item(base,source_location.parent)
        local preview_parent=locate_document_item(preview,source_location.parent)
        if original_parent and preview_parent and original_parent.item.widget=="pill"
            and preview_parent.item.slot and original_parent.item.slot then
            preview_parent.item.slot.span=original_parent.item.slot.span
        end
    end
    local regions={}
    local target_root=top_bar_root(preview,placeholder.iid)
    if target_root and target_root.slot then regions[target_root.slot.region]=true end
    local source_root=source_location and top_bar_root(preview,source.iid)
    if source_root and source_root.slot then regions[source_root.slot.region]=true end
    for region in regions do
        if not reflow_preview_region(preview,region,source.iid) then return clone(base) end
    end
    return preview
end
local function capture_drag_base(s,event,source,context)
    local original=document(s,context)
    if type(original)~="table" then return nil end
    local base=clone(original)
    local needs_bar=source.form=="bar" or source.host=="top_bar" or event.destination=="bar"
    if needs_bar and base.hosts.top_bar and base.hosts.top_bar.mode=="content"
        and not tile_document(base,context) then return nil end
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
        local has_live_form=false
        for _,item in group_items(doc,group,id) do
            local item_form=item.host=="canvas" and "desktop" or "bar"
            if item_form==form then
                has_live_form=true
                item.style=style_name
                if item.host=="canvas" then item.slot.w=style.preferred_span
                elseif not resize_bar(doc,item,style,req) then s["widgets.error"]="bar has no room for this style"; return end
            end
        end
        if form=="bar" and not has_live_form then grow_saved_bar_placement(form_state,style) end
        history(s,previous)
        finish(s,doc); return
    elseif kind~="toggle-form" then
        s["widgets.error"]="unknown widget operation"; return
    end
    local enabled=req.enabled==true
    if enabled==form_state.enabled then return end
    -- All validation and form availability checks happen before detaching a live placement.
    if enabled and form=="desktop" and not descriptor.styles then s["widgets.error"]="desktop form unavailable"; return end
    local existing=group_items(doc,group,id)
    if enabled then
        local duplicate=false
        for _,item in existing do if (item.host=="canvas")== (form=="desktop") then duplicate=true end end
        if not duplicate then
            local iid=form=="desktop" and (group..":desktop") or group
            local collision
            each_list(doc,function(item) if item.iid==iid then collision=true end end)
            if collision then s["widgets.error"]="widget instance id already exists"; return end
            local item={iid=iid,widget=id,group=group,settings=clone(state.settings or package.defaults or {}),style=style_name}
            local placement=form_state.placement
            if placement and placement.slot then placement=placement end
            if form=="desktop" then
                add_desktop(doc,item,placement,style,req)
            else
                if not insert_bar(doc,item,placement,style) then s["widgets.error"]="bar has no room for this widget"; return end
            end
        end
    else
        local target
        for _,item in existing do
            if (item.host=="canvas")== (form=="desktop") then target=item; break end
        end
        if not target then form_state.enabled=false; finish(s,doc); return end
        if form=="bar" and target.children and #target.children>0 then
            if not promote_children(doc,target) then s["widgets.error"]="bar has no room for pill children"; return end
        end
        local placement={host=target.host,slot=clone(target.slot)}
        local list=target.host=="canvas" and doc.hosts.canvas.instances or doc.hosts.top_bar.instances
        local removed=remove_identity(list,target.iid)
        if not removed then
            local function remove_nested(items)
                for _,item in items or {} do
                    local nested=remove_identity(item.children or {},target.iid)
                    if nested then return nested end
                end
            end
            removed=remove_nested(doc.hosts.top_bar.instances)
        end
        form_state.placement=placement
    end
    history(s,previous)
    form_state.enabled=enabled
    form_state.style=style_name
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
            s["widgets.drag.layout"]={accepted and (source.kind=="resize" and candidate or transient_layout(base,candidate,moved,source,s["widgets.drag.epoch"])) or clone(base)}
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
            local doc=(s["widgets.draft"] or {})[1]
            local live=moved
            if live and live.host=="canvas" then
                s["widgets.drag.snap_x"]=tonumber(live.slot.x) or 0
                s["widgets.drag.snap_y"]=tonumber(live.slot.y) or 0
            elseif live and live.slot then
                local host=doc.hosts.top_bar
                local region=host.regions[live.slot.region]
                local tiles=host.tiles or 1
                local position=(region and region.start or 0)+(live.slot.start or 0)
                if live.slot.region=="right" then position=tiles-position-(live.slot.span or 1) end
                s["widgets.drag.snap_x"]=position*((tonumber(event.screen_width) or tonumber(context.screen_width) or tiles)/tiles)
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
        ["window.width"]=1200,["window.height"]=800,["window.min_width"]=760,["window.min_height"]=540,
        ["window.preferred_min_width"]=800,["window.preferred_min_height"]=600,
        ["window.width_fraction"]=0.88,["window.height_fraction"]=0.86,
        ["widgets.layout"]={},["widgets.rev"]=0,["widgets.category"]="all",["widgets.search"]="",
        ["widgets.selected"]="",["widgets.group"]="",["widgets.form"]="bar",["widgets.scroll"]=0,["widgets.nav.scroll"]=0,
        ["widgets.screen_width"]=1920,["widgets.screen_height"]=1080,
        ["widgets.screen_origin_x"]=0,["widgets.screen_origin_y"]=0,["widgets.screen_id"]="",["widgets.outputs"]={},
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
        ["bead.desktop.clock"]="",["bead.desktop.clock.seconds"]="",["bead.desktop.clock.date"]="",
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
