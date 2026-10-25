-- SPDX-License-Identifier: GPL-3.0-only
return {stores={["widgets.layout"]={},["widgets.rev"]=0,["bar.height"]=50,["bar.notifications.count"]=0,["bar.launcher"]=false,["bar.popouts"]=false,["bar.reconnecting"]=false,["bar.workspace.enabled"]=true,["bar.tray.enabled"]=true,["bar.window.focus.enabled"]=true,["bar.workspace.origins"]={},["bar.dashboard.enabled"]=true,
    ["bar.tray.visible"]=false,["bar.workspace.count"]=6,["bar.workspace.icons"]={},["bar.workspace.hints"]=true,["bar.clock.twelve"]=false,["bar.updates.count"]=0,["bar.updates.enabled"]=true,
    ["bead.edge.dwell"]=0,["bead.edge.fade"]=0,["status.hovered"]="",["status.peek.visible"]=false,["status.peek.kind"]="",
    ["status.panel.kind"]=""},actions={["launcher.open"]=function(_) end},computed={
    -- desktop.clockFormat: the daemon clock is "HH:MM"; 12-hour shows "h:MM AM/PM".
    ["bar.clock.text"]=function(s)
        local c=s["bead.desktop.clock"]
        local h,m=string.match(c,"^(%d+):(%d%d)$")
        if not s["bar.clock.twelve"] or not h then return c end
        local n=tonumber(h)
        return string.format("%d:%s %s",(n+11)%12+1,m,n<12 and "AM" or "PM")
    end,
    ["bar.updates.text"]=function(s) return tostring(s["bar.updates.count"]) end,
    ["bar.clock.width"]=function(s) return s["bar.clock.twelve"] and 66 or 44 end,
    ["bar.title.cue.visible"]=function(s) return s["bead.edge.fade"]>0 end,
    ["bar.title.cue.fade"]=function(s) return s["bead.edge.fade"] end,
    ["bar.title.cue.halo"]=function(s) return .025*s["bead.edge.dwell"]*s["bead.edge.fade"] end,
    ["bar.title.cue.body"]=function(s) return (.03+.09*s["bead.edge.dwell"])*s["bead.edge.fade"] end,
    ["bar.title.cue.border"]=function(s) return (.06+.18*s["bead.edge.dwell"])*s["bead.edge.fade"] end,
    ["bar.notifications.text"]=function(s) return tostring(math.min(999,s["bar.notifications.count"])) end,
    ["bar.notifications.tail"]=function(s) return 7+7.5*#tostring(math.min(999,s["bar.notifications.count"])) end,
    ["bar.notifications.detail.width"]=function(s) return 1+8*#s["bar.notifications.text"] end,
    ["bar.battery.detail.width"]=function(s) return 1+8*#s["bead.desktop.battery.value"] end,
    -- QML NacreActiveTitle: with no focused window the title reads "Desktop".
    ["bar.title"]=function(s) local t=s["bead.focused_title"] return t~="" and t or tr("bead.bar.desktop-title") end,
}}
