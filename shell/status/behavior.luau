-- SPDX-License-Identifier: GPL-3.0-only
return {stores={
    ["panel.open"]=true,["view.edge"]="top",["status.ready"]=false,["status.active"]=false,
    ["status.heading"]="Status",["status.icon"]="info",["status.primary"]="Status unavailable",
    ["status.kind"]="network",["status.secondary"]="Status data unavailable",["status.detail"]="",
},actions={
    close=function(s) s["panel.open"]=false end,
    -- The controlled Runtime authorizes and handles this host action.
    ["network.toggle_wifi"]=function(_) end,
    ["network.connect"]=function(_) end,
    ["network.disconnect"]=function(_) end,
    ["bluetooth.toggle_power"]=function(_) end,
},computed={}}
