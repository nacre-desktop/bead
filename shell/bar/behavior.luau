-- SPDX-License-Identifier: GPL-3.0-only
return {stores={["bar.launcher"]=false,["bar.popouts"]=false,["bar.reconnecting"]=false,["bar.workspace.enabled"]=true,["bar.tray.enabled"]=true,["bar.dashboard.enabled"]=true,
    ["status.hovered"]="",["status.peek.visible"]=false,["status.peek.kind"]="",
    ["status.panel.kind"]=""},actions={["launcher.open"]=function(_) end},computed={
    ["bar.title.width"]=function(s) return math.min(550,#s["bead.focused_title"]*9.6) end,
}}
