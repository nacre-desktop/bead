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
local behavior={stores={
    ["panel.open"]=true,["view.edge"]="right",
    ["cc.back_focus"]="control-center.detail.header.back",["cc.detail_focus_prefix"]="control-center.detail.",
    ["cc.page"]="home",["cc.detail"]="",["cc.scroll"]=0,["cc.max_height"]=800,
    ["cc.src"]={},["cc.views"]={},["cc.saved"]={},["cc.saved_known"]=false,["cc.writable"]=false,["cc.message"]="",
    ["cc.battery"]="",["cc.battery_icon"]="battery_charging_full",["cc.battery_estimate"]="",
    ["cc.notices"]={},["cc.notice_count"]=0,["cc.notice_ready"]=false,["cc.notice_clear"]=false,
    ["cc.vpn"]={},["cc.vpn_message"]="",
    ["cc.edited"]=false,["cc.order"]={},["cc.drag"]="",
},actions={
    close=function(s) s["panel.open"]=false s["network.secret"]="" s["bluetooth.secret"]="" end,
    -- The control-center runtime authorizes and handles this host action (verb:source[:argument]).
    ["cc.do"]=function(_) end,
    page=function(s,name) s["network.secret"]="" s["bluetooth.secret"]="" s["cc.page"]=name s["cc.drag"]="" s["cc.scroll"]=0 end,
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

-- Bindings used by the shared detail builders; service flows live in the adapters.
for key,value in pairs({
    ["view.width"]=0,["view.height"]=0,["view.duration"]=0,
    ["network.health_message"]="",["network.secret"]="",["network.hidden_ssid"]="",["network.hidden_security"]="wpa_psk",["network.reveal"]=false,
    ["bluetooth.secret"]="",["bluetooth.reveal"]=false,["battery.level"]=0,
    ["status.primary"]="",["status.secondary"]="",["status.detail"]="",["cc.audio_devices"]={},
}) do behavior.stores[key]=value end
for _,name in ipairs({"network.toggle_wifi","network.connect","network.disconnect","network.open","network.cancel",
    "network.join","network.forget_ask","network.forget","bluetooth.toggle_power","bluetooth.pair","bluetooth.scan",
    "bluetooth.link","bluetooth.pair_respond","bluetooth.pair_cancel","bluetooth.pair_retry"}) do
    behavior.actions[name]=function(_) end
end
behavior.actions["network.toggle_reveal"]=function(s) s["network.reveal"]=not s["network.reveal"] end
behavior.actions["bluetooth.toggle_reveal"]=function(s) s["bluetooth.reveal"]=not s["bluetooth.reveal"] end
behavior.actions["network.set_security"]=function(s,id)
    if id=="open" or id=="wpa_psk" or id=="sae" then s["network.hidden_security"]=id end
end
return behavior
