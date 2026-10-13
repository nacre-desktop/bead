-- Test-only nested compositor: no autostart applications or live config imports.
hl.monitor({ output = "", mode = "1920x1080@60", position = "auto", scale = 1 })
hl.config({ ["animations.enabled"] = false, ["misc.disable_splash_rendering"] = true, ["misc.disable_hyprland_logo"] = true })
