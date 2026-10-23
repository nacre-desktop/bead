-- SPDX-License-Identifier: GPL-3.0-only
return {stores={["bar.height"]=50,["bar.notifications.count"]=0,["bar.launcher"]=false,["bar.popouts"]=false,["bar.reconnecting"]=false,["bar.workspace.enabled"]=true,["bar.tray.enabled"]=true,["bar.window.focus.enabled"]=true,["bar.workspace.origins"]={},["bar.dashboard.enabled"]=true,
    ["bar.workspace.count"]=6,["bar.workspace.icons"]={},["bar.workspace.hints"]=true,
    ["status.hovered"]="",["status.peek.visible"]=false,["status.peek.kind"]="",
    ["status.panel.kind"]=""},actions={["launcher.open"]=function(_) end},computed={
    ["bar.notifications.text"]=function(s) return tostring(math.min(999,s["bar.notifications.count"])) end,
    ["bar.notifications.tail"]=function(s) return 7+7.5*#tostring(math.min(999,s["bar.notifications.count"])) end,
    -- QML NacreActiveTitle: with no focused window the title reads "Desktop".
    ["bar.title"]=function(s) local t=s["bead.focused_title"] return t~="" and t or tr("bead.bar.desktop-title") end,
    ["bar.title.width"]=function(s) local t=s["bead.focused_title"] return math.min(550,#(t~="" and t or tr("bead.bar.desktop-title"))*9.6) end,
}}
