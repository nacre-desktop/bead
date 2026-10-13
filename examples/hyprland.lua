-- Nested development only. Never install as the session configuration.
hl.monitor({ output = "", mode = "1920x1080@60", position = "auto", scale = 1.5 })
hl.config({ ["animations.enabled"] = false, ["misc.disable_splash_rendering"] = true, ["misc.disable_hyprland_logo"] = true })
hl.on("hyprland.start", function() hl.exec_cmd([[exec nacre-shell --namespace bead-dev --ui ui/main.luau >> "${BEAD_PREVIEW_LOG:-/dev/null}" 2>&1]]) end)
