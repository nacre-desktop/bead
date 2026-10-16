-- SPDX-License-Identifier: GPL-3.0-only
return {stores={["bar.launcher"]=false,["bar.reconnecting"]=false},actions={["launcher.open"]=function(_) end},computed={
    ["bar.title.width"]=function(s) return math.min(550,#s["bead.focused_title"]*9.6) end,
}}
