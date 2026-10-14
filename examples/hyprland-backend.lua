-- Private nested backend indicator/title preview. Never install as session config.
hl.monitor({ output = "", mode = "1920x1080@60", position = "auto", scale = 1 })
hl.config({ ["animations.enabled"] = false, ["misc.disable_splash_rendering"] = true, ["misc.disable_hyprland_logo"] = true })
hl.on("hyprland.start", function() hl.exec_cmd([[exec nacre-shell --namespace bead-dev-backend --ui backend/main.luau --backend auto --no-keyboard-focus]]) end)
hl.on("hyprland.start", function() hl.exec_cmd([[exec quickshell --path spikes/backend-windows.qml]]) end)
