-- SPDX-License-Identifier: MIT
return {
    stores = {
        ["desktop.clock"] = "",
        ["desktop.battery.percent"] = -1,
        ["desktop.battery.available"] = false,
        ["desktop.audio.available"] = false,
        ["desktop.audio.volume"] = -1,
        ["desktop.audio.muted"] = false,
        ["commands.kernel.stdout"] = "",
        ["commands.kernel.ok"] = false,
        ["commands.kernel.error"] = "",
    },
    actions = {},
    computed = {
        ["status.battery"] = function(s)
            if s["desktop.battery.available"] then
                return "Battery " .. tostring(s["desktop.battery.percent"]) .. "%"
            end
            return "Battery unavailable"
        end,
        ["status.volume"] = function(s)
            if s["desktop.audio.available"] then
                return "Volume " .. tostring(s["desktop.audio.volume"])
            end
            return "Audio unavailable"
        end,
        ["status.mute"] = function(s)
            return s["desktop.audio.muted"] and "Unmute" or "Mute"
        end,
        ["status.kernel"] = function(s)
            return "Kernel " .. s["commands.kernel.stdout"]
        end,
    },
}
