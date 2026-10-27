-- SPDX-License-Identifier: GPL-3.0-only
-- Widget editor model. Markup validates the staged document
-- at the transaction boundary and rejects invalid transactions atomically.
local HISTORY=8
local function clone(value)
    return type(value)=="table" and copy_value(value) or value
end
local function document(s,request)
    local draft=s["widgets.draft"]
    return clone(draft[1] or request.document)
end
local function find(list,iid)
    for index,item in list do
        if item.iid==iid then return item,list,index end
        if item.children then
            local found,parent,position=find(item.children,iid)
            if found then return found,parent,position end
        end
    end
end
local function find_doc(doc,iid)
    for _,layout in doc.hosts do
        local found,parent,index=find(layout.instances or {},iid)
        if found then return found,parent,index,layout end
    end
end
local function children_of(doc,host)
    if host=="top_bar" then return doc.hosts.top_bar.instances end
    if host=="canvas" then return doc.hosts.canvas and doc.hosts.canvas.instances end
    local parent=find_doc(doc,host)
    if parent and parent.widget=="pill" then
        parent.children=parent.children or {}
        return parent.children
    end
end
local function contains(item,iid)
    if item.iid==iid then return true end
    for _,child in item.children or {} do if contains(child,iid) then return true end end
    return false
end
local function host_tiles(doc,host)
    if host~="canvas" then return doc.hosts.top_bar.tiles end
end
local function reflow(list,region,from,delta)
    for _,item in list do
        if item.slot.region==region and item.slot.start>=from then
            item.slot.start+=delta
        end
    end
end
local function remove(doc,iid)
    local item,parent,index=find_doc(doc,iid)
    if not item then return nil end
    local region,start,span=item.slot.region,item.slot.start,item.slot.span
    table.remove(parent,index)
    if region and start and span then reflow(parent,region,start+span,-span) end
    return item
end
local function place(doc,item,host,region,start,span,request)
    if host=="canvas" then
        local layout=doc.hosts.canvas
        if not layout then
            layout={width=request.canvas_width,height=request.canvas_height,instances={}}
            doc.hosts.canvas=layout
        end
        if type(layout.width)~="number" or type(layout.height)~="number" then return false,"canvas dimensions are unavailable" end
        local x,y,w,h=request.x,request.y,request.w,request.h
        if type(x)~="number" or type(y)~="number" or type(w)~="number" or type(h)~="number"
            or x~=x or y~=y or w~=w or h~=h then return false,"invalid canvas destination slot" end
        if request.snap then
            local step=type(request.snap)=="number" and request.snap or 32
            if step<=0 or step>20000 then return false,"invalid canvas snap" end
            x=math.floor(x/step+.5)*step; y=math.floor(y/step+.5)*step
        end
        x=math.min(math.max(0,x),layout.width-w); y=math.min(math.max(0,y),layout.height-h)
        if x<0 or y<0 or w<=0 or h<=0 or x+w>layout.width or y+h>layout.height then
            return false,"invalid canvas destination slot"
        end
        item.host="canvas"; item.slot={x=x,y=y,w=w,h=h}
        table.insert(layout.instances,item)
        return true
    end
    local list=children_of(doc,host)
    local tiles=host_tiles(doc,host)
    local layout=doc.hosts.top_bar
    if not list or not tiles then return false,"invalid destination host" end
    local bounds=host=="top_bar" and layout.regions[region] or {start=0,span=tiles}
    if not bounds or type(start)~="number" or type(span)~="number"
        or start<0 or span<1 or start%1~=0 or span%1~=0 or start+span>bounds.span then
        return false,"invalid destination slot"
    end
    for _,existing in list do
        if existing.slot.region==region then
            local first,last=existing.slot.start,existing.slot.start+existing.slot.span
            if first<start and start<last then return false,"destination must be a tile boundary" end
            if first>=start and last+span>bounds.span then return false,"destination region is full" end
        end
    end
    reflow(list,region,start,span)
    item.host=host; item.slot={region=region,start=start,span=span}
    table.insert(list,item)
    return true
end
local function commit(s,doc,req)
    local history=clone(s["widgets.undo"] or {})
    local old=s["widgets.draft"][1] or req.document
    if old then
        table.insert(history,{document=clone(old)})
        while #history>HISTORY do table.remove(history,1) end
    end
    s["widgets.undo"]=history
    s["widgets.draft"]={doc}; s["widgets.error"]=""
    return true
end
local function tile_document(doc,req)
    local host=clone(doc.hosts.top_bar)
    if host.mode~="content" or not req.geometry or #req.geometry==0 then return end
    local geometry,reservations={},{}
    for _,item in req.reservations or {} do reservations[item.iid]=item.width end
    for _,rect in req.geometry do geometry[rect.iid]=rect end
    local width=req.canvas_width
    if type(width)~="number" or width<=0 then return end
    local tile=width/host.tiles
    for _,region in {"left","centre","right"} do
        local items={}
        for _,item in host.instances do if item.slot.region==region then table.insert(items,item) end end
        table.sort(items,function(a,b) return a.slot.start<b.slot.start end)
        local previous=0
        for _,item in items do
            local rect=geometry[item.iid]
            local span=math.max(1,math.ceil((reservations[item.iid] or (rect and rect.w) or (item.slot.span*tile))/tile))
            local position=rect and (region=="right" and width-rect.x-rect.w or rect.x)
            if rect and region=="centre" then position=rect.x+rect.w/2-span*tile/2 end
            local start=position and math.floor(position/tile) or item.slot.start
            start=math.max(previous,start)
            if start+span>host.regions[region].span then return end
            item.slot.start=start; item.slot.span=span; previous=start+span
        end
    end
    host.mode="tiles"; doc.hosts.top_bar=host; doc.preset="Custom"
end
local function handle(s,args)
    local req=args and args[1]
    if type(req)~="table" then s["widgets.error"]="missing edit request"; return end
    local op=req.op
    if op=="begin" or op=="page" then s["widgets.context"]=false; s["widgets.editing"]=true; s["widgets.page"]=req.page or "layout"; return end
    if op=="menu" then
        local key="choice:"..req.key
        s["widgets.select"]=s["widgets.select"]==key and "" or key; return
    end
    if #s["widgets.draft"]==0 then s["widgets.base_rev"]=req.rev or s["widgets.rev"] end
    if op=="cancel" then
        s["widgets.draft"]={}; s["widgets.undo"]={}; s["widgets.editing"]=false
        s["widgets.error"]=""; return
    end
    if op=="done" then
        local doc=(s["widgets.draft"] or {})[1] or req.document
        if not doc then s["widgets.error"]="missing layout document"; return end
        s["widgets.draft"]={clone(doc)}; s["widgets.write"]={clone(doc)}; s["widgets.editing"]=false
        s["widgets.error"]=""; return
    end
    if op=="undo" then
        local history=clone(s["widgets.undo"] or {})
        local previous=table.remove(history)
        if previous then
            s["widgets.draft"]={clone(previous.document)}; s["widgets.undo"]=history; s["widgets.error"]=""
        end
        return
    end
    if op=="reset" then
        local doc=req.initial or req.document
        if not doc then s["widgets.error"]="missing default layout"; return end
        commit(s,clone(doc),req); return
    end
    local doc=document(s,req)
    if not doc then s["widgets.error"]="missing layout document"; return end
    if op=="field" then
        s["widgets.selected"]=req.iid or ""; s["widgets.select"]=req.key or ""
        s["widgets.field"]=tostring(req.value or ""); return
    elseif op=="select" then
        s["widgets.selected"]=req.iid or ""; s["widgets.field"]=""; s["widgets.select"]=""; s["widgets.page"]="settings"
        return
    elseif op=="setting" then
        if not req.iid and req.key=="dropdown_trigger" then
            assert(req.value=="click" or req.value=="hover","invalid dropdown trigger")
            doc.dropdown_trigger=req.value; commit(s,doc,req); return
        end
        local item=find_doc(doc,req.iid)
        if not item or type(req.key)~="string" then s["widgets.error"]="invalid widget setting target"; return end
        item.settings=item.settings or {}; item.settings[req.key]=req.from_field and s["widgets.field"] or clone(req.value)
        s["widgets.select"]=""
    elseif op=="remove" then
        if not remove(doc,req.iid) then s["widgets.error"]="widget not found"; return end
    elseif op=="move" then
        tile_document(doc,req)
        local item=find_doc(doc,req.iid)
        if not item then s["widgets.error"]="widget not found"; return end
        if contains(item,req.host) then s["widgets.error"]="cannot move a widget into itself or a descendant"; return end
        item=remove(doc,req.iid)
        local target=req.host or "top_bar"
        if target=="top_bar" then req.start=math.min(req.start or 0,doc.hosts.top_bar.regions[req.region or "left"].span-(req.span or item.slot.span or 1)) end
        local ok,message=place(doc,item,target,req.region or "left",req.start or 0,req.span or item.slot.span or 1,req)
        if not ok then s["widgets.error"]=message; return end
    elseif op=="add" then
        if type(req.widget)~="string" or #req.widget==0 or #req.widget>128 then s["widgets.error"]="invalid widget id"; return end
        local base="widget-"..req.widget:gsub("[^%w_-]",""):sub(1,64)
        local iid=base; local suffix=1
        while find_doc(doc,iid) do suffix+=1; iid=base.."-"..suffix end
        local host=req.host or "top_bar"
        local item={iid=iid,widget=req.widget,host=host,settings=clone(req.settings or {})}
        if host=="canvas" then item.slot={x=req.x,y=req.y,w=req.w,h=req.h}
        else item.slot={region=req.region or "left",start=req.start or 0,span=req.span or 1} end
        local ok,message=place(doc,item,host,item.slot.region,item.slot.start,item.slot.span,req)
        if not ok then s["widgets.error"]=message; return end
        s["widgets.selected"]=iid
    else
        s["widgets.error"]="unknown edit operation"; return
    end
    commit(s,doc,req)
end

return {
    stores={
        ["widgets.preview"]={},
        ["bead.surface.x"]=0,["bead.surface.y"]=0,
        ["widgets.layout"]={},["widgets.rev"]=0,["widgets.base_rev"]=0,["widgets.draft"]={},["widgets.geometry"]={},["widgets.undo"]={},
        ["widgets.write"]={},["widgets.error"]="",["widgets.message"]="",["widgets.selected"]="",["widgets.field"]="",
        ["widgets.context"]=false,["widgets.page"]="layout",["widgets.canvas.width"]=0,["widgets.canvas.height"]=0,
        ["widgets.drag.active"]=false,["widgets.drag.source"]={},["widgets.drag.phase"]="",["widgets.drag.epoch"]=0,
        ["widgets.drag.layout"]={},["widgets.drag.base"]={},["widgets.drag.original"]={},["widgets.drag.destination"]="",
        ["widgets.drag.x"]=0,["widgets.drag.y"]=0,["widgets.drag.start_x"]=0,["widgets.drag.start_y"]=0,
        ["widgets.drag.width"]=1,["widgets.drag.height"]=1,["widgets.drag.grab_x"]=0,["widgets.drag.grab_y"]=0,
        ["widgets.drag.snap_x"]=0,["widgets.drag.snap_y"]=0,
        ["widgets.drag.source_origin_x"]=0,["widgets.drag.source_origin_y"]=0,["widgets.drag.source_origin_valid"]=false,
        ["widgets.drag.target_origin_x"]=0,["widgets.drag.target_origin_y"]=0,
        ["widgets.drag.app_origin_x"]=0,["widgets.drag.app_origin_y"]=0,["widgets.drag.app_origin_valid"]=false,
        ["widgets.drag.app_width"]=0,["widgets.drag.app_height"]=0,["widgets.drag.target_output_id"]="",
        ["widgets.app.open"]=false,["widgets.screen_width"]=0,["widgets.screen_height"]=0,
        ["widgets.screen_origin_x"]=0,["widgets.screen_origin_y"]=0,["widgets.screen_id"]="",["widgets.outputs"]={},
        ["launcher.apps"]={},["dashboard.host"]={},["bluetooth.devices"]={},["bluetooth.power_checked"]=false,
        ["network.rows"]={},["network.wifi_enabled"]=false,["notifications.rows"]={},
        ["status.updates_checked"]="Never checked",["status.updates_can_open"]=false,["status.updates_can_refresh"]=false,
        ["session.enabled"]=false,["session.busy"]=false,["session.confirm"]="",["session.confirm_action"]="",
        ["session.message"]="",
        ["widgets.group"]="",
        ["widgets.select"]="",["widgets.scroll"]=0,["widgets.editing"]=true,["panel.open"]=true,
        ["view.width"]=0,["view.height"]=0,["view.duration"]=0,
        -- Bar defaults needed by widgets/context during catalogue live previews.
        ["bar.height"]=50,["bar.launcher"]=false,["bar.popouts"]=false,["bar.reconnecting"]=false,
        ["bar.workspace.enabled"]=true,["bar.tray.enabled"]=true,["bar.window.focus.enabled"]=true,
        ["bar.workspace.origins"]={},["bar.dashboard.enabled"]=true,["bar.tray.visible"]=false,
        ["bar.workspace.count"]=6,["bar.workspace.icons"]={},["bar.workspace.hints"]=true,
        ["bar.clock.twelve"]=false,["bar.updates.count"]=0,["bar.updates.enabled"]=true,
        ["bar.notifications.count"]=0,["bead.edge.dwell"]=0,["bead.edge.fade"]=0,
        ["status.hovered"]="",["status.peek.visible"]=false,["status.peek.kind"]="",["status.panel.kind"]="",
        ["bead.focused_title"]="",["bead.windows"]={},["bead.workspaces"]={},
        ["bead.desktop.audio.percent"]=0,["bead.desktop.audio.device"]="Output unavailable",
        ["bead.desktop.audio.label"]="Unavailable",["bead.desktop.titlevisible"]=true,
        ["bead.desktop.clock"]="",["bead.desktop.clock.seconds"]="",["bead.desktop.clock.date"]="",
        ["bead.desktop.audio"]="volume_mute",["bead.desktop.audio.available"]=false,
        ["bead.desktop.battery"]="battery_unknown",["bead.desktop.battery.value"]="",
        ["bead.desktop.battery.status"]="",["bead.desktop.battery.remaining"]="",
        ["bead.desktop.battery.value.width"]=3,["bead.desktop.network"]="wifi_off",
        ["bead.desktop.bluetooth"]="bluetooth_disabled",["bead.desktop.tray"]={},["bead.desktop.menu"]={},
        ["bead.desktop.tray.tooltip"]="",["bead.desktop.tray.tooltip.visible"]=false,
    },
    actions={
        ["widgets.drag"]=function(s,args)
            local event=args[1]
            if event.phase=="move" then
                s["widgets.drag.active"]=true
                s["widgets.drag.x"]=event.x-(event.grab_x or 0); s["widgets.drag.y"]=event.y-(event.grab_y or 0)
                s["widgets.drag.source"]=type(event.source)=="string" and event.source or ("catalogue:"..event.source[1].widget)
                return
            end
            s["widgets.drag.active"]=false
            if event.phase~="drop" or event.target_action~="widgets.edit" then return end
            local target=event.target and event.target[1]
            if not target then return end
            target=clone(target)
            if type(event.source)=="string" then
                target.iid=event.source
                if target.op=="add" then target.op="move" end
            elseif type(event.source)=="table" and event.source[1] then
                target.widget=event.source[1].widget; target.op="add"
            else return end
            if target.point and target.host=="canvas" then
                target.x=math.max(0,event.x+(target.origin_x or 0)-(event.grab_x or 0))
                target.y=math.max(0,event.y+(target.origin_y or 0)-(event.grab_y or 0))
                target.w=math.max(1,event.width or target.w or 1)
                target.h=math.max(1,event.height or target.h or 1)
            elseif target.point then
                local width=target.host_width
                local doc=s["widgets.draft"][1] or target.document
                local span=type(event.source)=="string" and (find_doc(doc,event.source) or {}).slot
                span=span and span.span or 1
                local bounds=doc.hosts.top_bar.regions[target.region]
                local tile=width/doc.hosts.top_bar.tiles
                local x=event.x+(target.origin_x or 0)-bounds.start*tile
                if target.region=="right" then x=bounds.span*tile-x end
                target.start=math.max(0,math.min(doc.hosts.top_bar.regions[target.region].span-span,math.floor((x-(event.width or 0)/2)*doc.hosts.top_bar.tiles/width)))
            end
            handle(s,{target})
        end,
        ["widgets.edit"]=handle,
        ["session.select"]=function(_) end,
        ["session.confirm"]=function(_) end,
        ["session.cancel"]=function(_) end,
        ["launcher.open"]=function(_) end,
        ["menu.activate"]=function(_) end,
        ["notifications.dismiss"]=function(_) end,
        ["updates.open"]=function(_) end,
        ["services.audio"]=function(_) end,
        ["bluetooth.toggle_power"]=function(_) end,
        ["view.open"]=function(_) end,
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
    },
}
