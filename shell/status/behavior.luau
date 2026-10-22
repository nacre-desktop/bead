-- SPDX-License-Identifier: GPL-3.0-only
return {stores={
    ["panel.open"]=true,["view.edge"]="top",["view.width"]=310,["view.height"]=222,["view.duration"]=0.2,
    ["status.ready"]=false,["status.active"]=false,
    ["status.heading"]=tr("bead.panel.status-heading"),["status.icon"]="info",["status.primary"]=tr("bead.panel.status-none"),
    ["status.kind"]="network",["status.secondary"]=tr("bead.panel.status-data-unavailable"),["status.detail"]="",
},actions={
    close=function(s) s["panel.open"]=false end,
    -- The controlled Runtime authorizes and handles this host action.
    ["network.toggle_wifi"]=function(_) end,
    ["network.connect"]=function(_) end,
    ["network.disconnect"]=function(_) end,
    ["bluetooth.toggle_power"]=function(_) end,
},computed={}}
