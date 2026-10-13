-- Nested development only. Never install as the session configuration.
hl.monitor({ output = "", mode = "1920x1080@60", position = "auto", scale = 1 })
hl.config({ ["animations.enabled"] = false, ["misc.disable_splash_rendering"] = true, ["misc.disable_hyprland_logo"] = true })
hl.on("hyprland.start", function() hl.exec_cmd("nacre-shell --namespace bead-dev --animate") end)
