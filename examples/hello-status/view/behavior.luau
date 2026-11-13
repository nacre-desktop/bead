-- SPDX-License-Identifier: MIT
return {
    stores = {},
    actions = {},
    computed = {
        ["status.workspaces"] = function(s)
            local rows = {}
            for _, workspace in ipairs(s["desktop.workspaces"]) do
                table.insert(rows, {
                    id = workspace.id,
                    label = workspace.name ~= "" and workspace.name or tostring(workspace.index),
                    fill = workspace.focused and "primary" or "surface",
                    color = workspace.focused and "onPrimary" or "onSurface",
                })
            end
            return rows
        end,
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
        ["status.media"] = function(s)
            if not s["desktop.media.available"] then return "Nothing playing" end
            local title = s["desktop.media.title"]
            if title == "" then title = s["desktop.media.player"] end
            if s["desktop.media.artist"] ~= "" then title = title .. " — " .. s["desktop.media.artist"] end
            return title
        end,
        ["status.media_button"] = function(s)
            return s["desktop.media.status"] == "Playing" and "Pause" or "Play"
        end,
        ["status.power"] = function(s)
            if not s["desktop.power.available"] then return "Power unavailable" end
            return "Power " .. s["desktop.power.profile"]
        end,
    },
}
