-- SPDX-License-Identifier: GPL-3.0-only
return {stores={
    ["panel.open"]=true,["view.edge"]="top",["view.width"]=310,["view.height"]=222,["view.duration"]=0.2,
    ["status.ready"]=false,["status.active"]=false,
    ["status.heading"]=tr("bead.panel.status-heading"),["status.icon"]="info",["status.primary"]=tr("bead.panel.status-none"),
    ["status.kind"]="network",
    -- Typed network fields. `*.secret` stores are never traced or saved; the host clears them after use.
    ["network.secret"]="",["network.hidden_ssid"]="",["network.hidden_security"]="wpa_psk",["network.reveal"]=false,
    -- Typed Bluetooth PIN or passkey (same convention); the host clears it after use.
    ["bluetooth.secret"]="",["bluetooth.reveal"]=false,
    ["status.secondary"]=tr("bead.panel.status-data-unavailable"),["status.detail"]="",
},actions={
    close=function(s) s["panel.open"]=false;s["network.secret"]="";s["bluetooth.secret"]="" end,
    -- The controlled Runtime authorizes and handles this host action.
    ["network.toggle_wifi"]=function(_) end,
    ["network.connect"]=function(_) end,
    ["network.disconnect"]=function(_) end,
    ["network.open"]=function(_) end,
    ["network.cancel"]=function(_) end,
    ["network.join"]=function(_) end,
    ["network.forget_ask"]=function(_) end,
    ["network.forget"]=function(_) end,
    ["network.toggle_reveal"]=function(s) s["network.reveal"]=not s["network.reveal"] end,
    ["network.set_security"]=function(s,id)
        if id=="open" or id=="wpa_psk" or id=="sae" then s["network.hidden_security"]=id end
    end,
    ["bluetooth.toggle_power"]=function(_) end,
    ["bluetooth.pair"]=function(_) end,
    ["bluetooth.pair_respond"]=function(_) end,
    ["bluetooth.pair_cancel"]=function(_) end,
    ["bluetooth.pair_retry"]=function(_) end,
    ["bluetooth.toggle_reveal"]=function(s) s["bluetooth.reveal"]=not s["bluetooth.reveal"] end,
},computed={}}
