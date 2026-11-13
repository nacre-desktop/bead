-- SPDX-License-Identifier: MIT
return {
    stores = {},
    actions = {},
    computed = {
        volume = function(s)
            if not s["desktop.audio.available"] then return "No audio" end
            if s["desktop.audio.muted"] then return "Muted" end
            return "Volume " .. tostring(math.floor(s["desktop.audio.volume"] * 100 + 0.5)) .. "%"
        end,
        mute = function(s) return s["desktop.audio.muted"] and "Unmute" or "Mute" end,
    },
}
