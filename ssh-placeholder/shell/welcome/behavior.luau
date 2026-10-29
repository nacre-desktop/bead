-- SPDX-License-Identifier: GPL-3.0-only
-- Welcome (B0059). The tour position is plain view state (`welcome.step`, not `*.page`: the shell would
-- save a `.page` store and reopen the window on it); the adapter (`welcome.rs`) projects the
-- window size, which apps this desktop can open and the shortcut rows.
return {stores={
    ["welcome.step"]="welcome",["welcome.guide"]="",["welcome.scroll"]=0,
    ["welcome.width"]=1100,["welcome.height"]=800,
    ["welcome.can.settings"]=true,["welcome.can.colors"]=false,["welcome.can.ai"]=false,["welcome.can.brain"]=false,
    ["welcome.shortcuts"]={},["welcome.demo"]={},["welcome.shortcuts.state"]="unavailable",
},actions={
    -- Go to a tour page (or the help page) and start it at the top.
    go=function(s,name) s["welcome.step"]=name s["welcome.guide"]="" s["welcome.scroll"]=0 end,
    -- Show or hide an optional app's setup guide on the apps page.
    guide=function(s,name) s["welcome.guide"]=name s["welcome.scroll"]=0 end,
    -- The Welcome adapter authorizes and handles this host action (route, link, close).
    ["welcome.do"]=function(_) end,
},computed={}}
