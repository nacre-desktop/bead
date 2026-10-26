-- SPDX-License-Identifier: GPL-3.0-only
-- Isolated state contract for the input-empty drag overlay canvas.
return {stores={
    ["widgets.layout"]={},["widgets.draft"]={},
    ["widgets.drag.layout"]={},["widgets.drag.base"]={},["widgets.drag.original"]={},
    ["widgets.preview"]={},
    ["widgets.editing"]=false,
    ["widgets.drag.active"]=false,["widgets.drag.source"]={},["widgets.drag.destination"]="",
    ["widgets.drag.phase"]="",["widgets.drag.epoch"]=0,["widgets.drag.snap_x"]=0,["widgets.drag.snap_y"]=0,
    ["widgets.drag.x"]=0,["widgets.drag.y"]=0,["widgets.drag.start_x"]=0,["widgets.drag.start_y"]=0,
    ["widgets.drag.width"]=1,["widgets.drag.height"]=1,["widgets.drag.grab_x"]=0,["widgets.drag.grab_y"]=0,
    ["widgets.drag.source_origin_x"]=0,["widgets.drag.source_origin_y"]=0,["widgets.drag.source_origin_valid"]=false,
    ["widgets.drag.target_origin_x"]=0,["widgets.drag.target_origin_y"]=0,
    ["widgets.drag.app_origin_x"]=0,["widgets.drag.app_origin_y"]=0,["widgets.drag.app_origin_valid"]=false,
    ["widgets.drag.app_width"]=0,["widgets.drag.app_height"]=0,
    ["widgets.drag.entry.target_width"]=0,["widgets.drag.entry.target_height"]=0,
    ["widgets.drag.target_output_id"]="",
    ["bar.height"]=50,["bar.launcher"]=false,["bar.popouts"]=false,["bar.reconnecting"]=false,
    ["bar.workspace.enabled"]=true,["bar.tray.enabled"]=true,["bar.window.focus.enabled"]=true,
    ["bar.workspace.origins"]={},["bar.dashboard.enabled"]=true,["bar.tray.visible"]=false,
    ["bar.workspace.count"]=0,["bar.workspace.icons"]={},["bar.workspace.hints"]=true,
    ["bar.clock.twelve"]=false,["bar.updates.count"]=0,["bar.updates.enabled"]=true,
    ["bar.notifications.count"]=0,["bead.edge.dwell"]=0,["bead.edge.fade"]=0,
    ["bead.windows"]={},["bead.workspaces"]={},["bead.focused_title"]="",
    ["bead.output.name"]="",
    ["bead.desktop.audio.percent"]=0,["bead.desktop.audio.device"]="Output unavailable",
    ["bead.desktop.audio.label"]="Unavailable",["bead.desktop.titlevisible"]=true,
    ["bead.desktop.clock"]="--:--",["bead.desktop.clock.seconds"]="--:--:--",["bead.desktop.clock.date"]="",
    ["bead.desktop.audio"]="volume_mute",["bead.desktop.audio.available"]=false,
    ["bead.desktop.battery"]="battery_unknown",["bead.desktop.battery.value"]="",
    ["bead.desktop.battery.value.width"]=3,["bead.desktop.network"]="wifi_off",
    ["bead.desktop.bluetooth"]="bluetooth_disabled",["bead.desktop.tray"]={},
    ["bead.desktop.menu"]={},["bead.desktop.tray.tooltip"]="",
    ["bead.desktop.tray.tooltip.visible"]=false,
},computed={
    ["bar.clock.text"]=function(s)
        local clock=s["bead.desktop.clock"]
        local hour,minute=string.match(clock,"^(%d+):(%d%d)$")
        if not s["bar.clock.twelve"] or not hour then return clock end
        local value=tonumber(hour)
        return string.format("%d:%s %s",(value+11)%12+1,minute,value<12 and "AM" or "PM")
    end,
    ["bar.updates.text"]=function(s) return tostring(s["bar.updates.count"]) end,
    ["bar.notifications.text"]=function(s) return tostring(math.min(999,s["bar.notifications.count"])) end,
    ["bar.notifications.tail"]=function(s) return 7+7.5*#tostring(math.min(999,s["bar.notifications.count"])) end,
    ["bar.notifications.detail.width"]=function(s) return 1+8*#s["bar.notifications.text"] end,
    ["bar.battery.detail.width"]=function(s) return 1+8*#s["bead.desktop.battery.value"] end,
    ["bar.title"]=function(s)
        local title=s["bead.focused_title"]
        return title~="" and title or tr("bead.bar.desktop-title")
    end,
    ["bar.title.cue.visible"]=function(s) return s["bead.edge.fade"]>0 end,
    ["bar.title.cue.fade"]=function(s) return s["bead.edge.fade"] end,
    ["bar.title.cue.halo"]=function(s) return .025*s["bead.edge.dwell"]*s["bead.edge.fade"] end,
    ["bar.title.cue.body"]=function(s) return (.03+.09*s["bead.edge.dwell"])*s["bead.edge.fade"] end,
    ["bar.title.cue.border"]=function(s) return (.06+.18*s["bead.edge.dwell"])*s["bead.edge.fade"] end,
}}
