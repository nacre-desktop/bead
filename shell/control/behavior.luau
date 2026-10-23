-- SPDX-License-Identifier: GPL-3.0-only
-- Control-center stores. The host projects `cc.src` (data sources), `cc.views` (panels it can open), `cc.saved`
-- (the stored tile order), `cc.writable`, `cc.message`, `cc.battery*` and `cc.max_height`. The view owns the
-- edit draft (`cc.order`, `cc.edited`): the host stores a draft that differs from what is saved.
-- Host lists are lists of tables, so an order is stored as {{id=...},...}; here it is a plain id list.
local function ids(list) local out={} for i,item in ipairs(list) do out[i]=item.id end return out end
local function split(joined) local out={} for id in string.gmatch(joined,"[^,]+") do table.insert(out,id) end return out end
local function index(list,id) for i,v in ipairs(list) do if v==id then return i end end return nil end
local function store(order) local out={} for i,id in ipairs(order) do out[i]={id=id} end return out end
local function edit(s,order) s["cc.order"]=store(order) s["cc.edited"]=true end
return {stores={
    ["panel.open"]=true,["view.edge"]="right",
    ["cc.page"]="home",["cc.detail"]="",["cc.scroll"]=0,["cc.max_height"]=800,
    ["cc.src"]={},["cc.views"]={},["cc.saved"]={},["cc.saved_known"]=false,["cc.writable"]=false,["cc.message"]="",
    ["cc.battery"]="",["cc.battery_icon"]="battery_full",
    ["cc.edited"]=false,["cc.order"]={},["cc.drag"]="",
},actions={
    close=function(s) s["panel.open"]=false end,
    -- The control-center runtime authorizes and handles this host action (verb:source[:argument]).
    ["cc.do"]=function(_) end,
    page=function(s,name) s["cc.page"]=name s["cc.drag"]="" s["cc.scroll"]=0 end,
    detail=function(s,id) s["cc.page"]="detail" s["cc.detail"]=id s["cc.scroll"]=0 end,
    -- Entering the editor copies the shown order into the draft; nothing is saved until a change.
    begin=function(s,joined)
        if not s["cc.edited"] then s["cc.order"]=store(split(joined)) end
        s["cc.page"]="edit" s["cc.drag"]="" s["cc.scroll"]=0
    end,
    -- A press starts a drag on that tile; moving over another tile moves the dragged one to its place.
    touch=function(s,id)
        if not s["bead.pressed"] or s["cc.drag"]=="" then s["cc.drag"]=id return end
        local order=ids(s["cc.order"])
        local from,to=index(order,s["cc.drag"]),index(order,id)
        if from and to and from~=to then
            table.remove(order,from)
            table.insert(order,to,s["cc.drag"])
            edit(s,order)
        end
    end,
    remove=function(s,id)
        local order=ids(s["cc.order"])
        local at=index(order,id)
        if at then table.remove(order,at) edit(s,order) end
    end,
    add=function(s,id)
        local order=ids(s["cc.order"])
        if not index(order,id) then table.insert(order,id) edit(s,order) end
        s["cc.page"]="edit"
    end,
    reset=function(s,joined) edit(s,split(joined)) end,
},computed={}}
