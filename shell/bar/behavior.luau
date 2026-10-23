-- SPDX-License-Identifier: GPL-3.0-only
return {stores={["bar.height"]=50,["bar.notifications.count"]=0,["bar.launcher"]=false,["bar.popouts"]=false,["bar.reconnecting"]=false,["bar.workspace.enabled"]=true,["bar.tray.enabled"]=true,["bar.window.focus.enabled"]=true,["bar.workspace.origins"]={},["bar.dashboard.enabled"]=true,
    ["bar.workspace.count"]=6,["bar.workspace.icons"]={},["bar.workspace.hints"]=true,["bar.clock.twelve"]=false,
    ["status.hovered"]="",["status.peek.visible"]=false,["status.peek.kind"]="",
    ["status.panel.kind"]=""},actions={["launcher.open"]=function(_) end},computed={
    -- desktop.clockFormat: the daemon clock is "HH:MM"; 12-hour shows "h:MM AM/PM".
    ["bar.clock.text"]=function(s)
        local c=s["bead.desktop.clock"]
        local h,m=string.match(c,"^(%d+):(%d%d)$")
        if not s["bar.clock.twelve"] or not h then return c end
        local n=tonumber(h)
        return string.format("%d:%s %s",(n+11)%12+1,m,n<12 and "AM" or "PM")
    end,
    ["bar.clock.width"]=function(s) return s["bar.clock.twelve"] and 66 or 48 end,
    ["bar.notifications.text"]=function(s) return tostring(math.min(999,s["bar.notifications.count"])) end,
    ["bar.notifications.tail"]=function(s) return 7+7.5*#tostring(math.min(999,s["bar.notifications.count"])) end,
    -- QML NacreActiveTitle: with no focused window the title reads "Desktop".
    ["bar.title"]=function(s) local t=s["bead.focused_title"] return t~="" and t or tr("bead.bar.desktop-title") end,
    ["bar.title.width"]=function(s) local t=s["bead.focused_title"] return math.min(550,#(t~="" and t or tr("bead.bar.desktop-title"))*8.7) end,
}}
