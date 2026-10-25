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
local function history(s,doc)
    local old=(s["widgets.draft"] or {})[1] or doc
    local undo=clone(s["widgets.undo"] or {})
    if old then
        table.insert(undo,{document=clone(old)})
        while #undo>LIMIT do table.remove(undo,1) end
    end
    s["widgets.undo"]=undo
end
local function finish(s,doc)
    s["widgets.draft"]={doc}
    s["widgets.error"]=""
    s["widgets.message"]=""
end
local function catalogue(req,id)
    for _,item in req.catalogue or {} do if item.id==id then return item end end
end
local function group_for(doc,id)
    for group,item in doc.widgets or {} do if item.widget==id then return group,item end end
    local found
    local function scan(items)
        for _,item in items or {} do
            if not found and item.widget==id then found=item end
            scan(item.children)
        end
    end
    for _,host in doc.hosts or {} do scan(host.instances) end
    local group=found and found.iid or "widget-"..id
    local state=doc.widgets and doc.widgets[group]
    if not state then
        state={widget=id,settings=clone(found and found.settings or {}),forms={}}
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
    local preferred=style.preferred_span or 1
    local height=style.preferred_height or 96
    local host=doc.hosts.canvas
    if not host then
        host={width=req.screen_width or 1920,height=req.screen_height or 1080,instances={}}
        doc.hosts.canvas=host
    end
    local slot=placement and placement.slot or {}
    local x=math.max(0,math.min(host.width-preferred,tonumber(slot.x) or 32))
    local y=math.max(0,math.min(host.height-height,tonumber(slot.y) or 32))
    item.host="canvas"; item.slot={x=x,y=y,w=preferred,h=tonumber(slot.h) or height}
    table.insert(host.instances,item)
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
local function resize_bar(doc,item,style)
    local current=item.slot and item.slot.span or style.preferred_span
    local span=current
    if span<style.min_span or span>style.max_span then span=style.preferred_span end
    if item.host~="top_bar" then item.slot.span=span; return true end
    local bounds=doc.hosts.top_bar.regions[item.slot.region]
    if not bounds then return false end
    local delta=span-current
    if delta>0 then
        if item.slot.start+span>bounds.span then return false end
        local following={}
        for _,other in doc.hosts.top_bar.instances do
            if other~=item and other.slot.region==item.slot.region and other.slot.start>=item.slot.start+current then
                table.insert(following,other)
            end
        end
        table.sort(following,function(a,b) return a.slot.start<b.slot.start end)
        local end_at=item.slot.start+span
        for _,other in following do
            if other.slot.start<end_at then other.slot.start=end_at end
            end_at=other.slot.start+other.slot.span
        end
        if end_at>bounds.span then return false end
    end
    item.slot.span=span
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
    local group,state=group_for(doc,id)
    local form=req.form or s["widgets.form"] or "bar"
    if form~="bar" and form~="desktop" then s["widgets.error"]="unsupported form"; return end
    local descriptor=package.forms and package.forms[form]
    if not descriptor then s["widgets.error"]="widget has no "..form.." form"; return end
    local form_state=state.forms[form] or {enabled=false,style=descriptor.default_style,placement={}}
    state.forms[form]=form_state
    local style_name=req.style or form_state.style
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
            local item_form=item.host=="canvas" and "desktop" or "bar"
            if item_form==form then
                item.style=style_name
                if item.host=="canvas" then item.slot.w=style.preferred_span
                elseif not resize_bar(doc,item,style) then s["widgets.error"]="bar has no room for this style"; return end
            end
        end
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

return {
    stores={
        ["window.width"]=1200,["window.height"]=800,["window.min_width"]=760,["window.min_height"]=540,
        ["window.preferred_min_width"]=800,["window.preferred_min_height"]=600,
        ["window.width_fraction"]=0.88,["window.height_fraction"]=0.86,
        ["widgets.layout"]={},["widgets.rev"]=0,["widgets.category"]="all",["widgets.search"]="",
        ["widgets.selected"]="",["widgets.form"]="bar",["widgets.scroll"]=0,["widgets.nav.scroll"]=0,
        ["widgets.screen_width"]=1920,["widgets.screen_height"]=1080,
        ["widgets.message"]="",["widgets.error"]="",["widgets.busy"]=false,["widgets.draft"]={},["widgets.undo"]={},
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
