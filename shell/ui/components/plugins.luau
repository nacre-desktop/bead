-- SPDX-License-Identifier: GPL-3.0-only
-- B0208: the launcher's `>plugin [git URL or folder]` mode. Installs, enables (showing the
-- access a plugin declares first, as the Plugins page does) and updates plugins; the host's
-- `plugins.do` action does the work and its rows come from the plugin loader.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local S=T.spacing
local body=T.text.body*4/3
local caption=T.text.caption*4/3
local function button(id,text,action,props)
    local p=props or {}
    p.id=id; p.text=text; p.on_activate=action; p.activate_on_release=true; p.focus_on_pointer=false
    p.height=S.huge; p.padding_left=S.md; p.padding_right=S.md; p.shrink=0; p.font_size=caption
    p.width=math.ceil(ui.text_width(text,"IBM Plex Sans",caption))+2*S.md
    return kit.Button(p)
end
local function act(argument) return ui.action("plugins.do",argument) end
return function()
    local source=ui.resolve_for_check(ui.bind("launcher.plugin.source")) or ""
    local rows=ui.resolve_for_check(ui.bind("launcher.plugin.rows")) or {}
    local notice=ui.resolve_for_check(ui.bind("plugins.message")) or ""
    if notice=="" then notice=ui.resolve_for_check(ui.bind("launcher.plugin.notice")) or "" end
    local column={id="plugin-mode",grow=1,gap=S.sm,
        ui.Text{id="plugin-title",text="Plugins",font_size=16,height=32},
        ui.Text{id="plugin-hint",text="Type >plugin followed by a git URL or an absolute folder path to install.",
            color="onSurfaceVariant",font_size=caption,height=18},
        ui.Text{id="plugin-trust",text="Plugins run Luau code with the access they declare. Only enable plugins you trust.",
            color="onSurfaceVariant",font_size=caption,height=18}}
    local install={id="plugin-install",width="fill",height=S.huge+S.sm,gap=S.sm,align="center",
        ui.Text{id="plugin-source",text=source~="" and "Install from "..source or "No source yet",width=0,grow=1,height=20,font_size=body,
            color=source~="" and "onSurface" or "onSurfaceVariant"},
        button("plugin-check","Check for updates",act("check"),{fill="surfaceContainerHigh"})}
    if source~="" then table.insert(install,button("plugin-install-button","Install",act("install|"..source),{fill="primaryContainer",color="onPrimaryContainer"})) end
    table.insert(column,ui.Row(install))
    if notice~="" then table.insert(column,ui.Text{id="plugin-notice",text=notice,font_size=caption,height=18,color="onSurfaceVariant"}) end
    for _,row in rows do
        local enable=row.enabled and button("disable","Disable",act("disable|"..row.id),{fill="surfaceContainerHigh"})
            or button("enable","Enable",act("enable|"..row.id),{fill="primaryContainer",color="onPrimaryContainer"})
        local line={id="plugin-"..row.id,width="fill",height=S.huge+S.xl,gap=S.sm,align="center",padding_left=S.sm,padding_right=S.sm,
            radius=T.radii.small,fill="surfaceContainer",
            ui.Column{id="copy",width=0,grow=1,gap=S.hairline,
                ui.Text{id="name",text=row.name,font_size=body,height=20},
                ui.Text{id="access",text=(row.error~="" and row.error) or ("Access: "..row.access),font_size=caption,height=16,
                    color=row.error~="" and "error" or "onSurfaceVariant"}}}
        if row.update then table.insert(line,button("update","Update",act("update|"..row.id),{fill="tertiaryContainer",color="onTertiaryContainer"})) end
        table.insert(line,enable)
        table.insert(column,ui.Row(line))
    end
    return ui.Column(column)
end
