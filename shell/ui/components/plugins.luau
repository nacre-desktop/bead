-- SPDX-License-Identifier: GPL-3.0-only
-- B0208: the launcher's `>plugin [git URL or folder]` mode. Installs, enables (showing the
-- access a plugin declares first, as the Plugins page does) and updates plugins; the host's
-- `plugins.do` action does the work and its rows come from the plugin loader.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local S=T.spacing
local Lines=require("./text-lines")
local M=require("./metrics")
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
    local width=M.content_width(ui.resolve_for_check(ui.bind("bead.viewport.width")),false)
    local column={id="plugin-mode",width=width,gap=S.sm,
        Lines{id="plugin-hint",text="Type >plugin followed by a git URL or an absolute folder path to install.",
            width=width,color="onSurfaceVariant",font_size=caption,line_height=18,max_lines=2},
        Lines{id="plugin-trust",text="Plugins run Luau code with the access they declare. Only enable plugins you trust.",
            width=width,color="onSurfaceVariant",font_size=caption,line_height=18,max_lines=2}}
    local check=button("plugin-check","Check for updates",act("check"),{fill="surfaceContainerHigh"})
    local add=source~="" and button("plugin-install-button","Install",act("install|"..source),{fill="primaryContainer",color="onPrimaryContainer"}) or nil
    local source_width=width-check.props.width-S.sm-(add and add.props.width+S.sm or 0)
    local source_line=Lines{id="plugin-source",text=source~="" and "Install from "..source or "No source yet",
        width=source_width,font_size=body,line_height=20,max_lines=1,color=source~="" and "onSurface" or "onSurfaceVariant"}
    local install={id="plugin-install",width=width,height=math.max(S.huge,source_line.props.height)+S.sm,gap=S.sm,align="center",source_line,check}
    if add then table.insert(install,add) end
    table.insert(column,ui.Row(install))
    if notice~="" then table.insert(column,Lines{id="plugin-notice",text=notice,width=width,font_size=caption,line_height=18,max_lines=2,color="onSurfaceVariant"}) end
    for _,row in rows do
        local enable=row.enabled and button("disable","Disable",act("disable|"..row.id),{fill="surfaceContainerHigh"})
            or button("enable","Enable",act("enable|"..row.id),{fill="primaryContainer",color="onPrimaryContainer"})
        local update=row.update and button("update","Update",act("update|"..row.id),{fill="tertiaryContainer",color="onTertiaryContainer"}) or nil
        local icon_size=T.icons.medium*4/3
        local copy_width=width-2*S.sm-icon_size-S.sm-enable.props.width-S.sm-(update and update.props.width+S.sm or 0)
        local name=Lines{id="name",text=row.name,width=copy_width,font_size=body,line_height=20,max_lines=1,color="onSurface"}
        local access=Lines{id="access",text=(row.error~="" and row.error) or ("Access: "..row.access),width=copy_width,
            font_size=caption,line_height=18,max_lines=2,color=row.error~="" and "error" or "onSurfaceVariant"}
        local line={id="plugin-"..row.id,width=width,height=name.props.height+access.props.height+S.hairline+2*S.sm,gap=S.sm,align="center",padding_left=S.sm,padding_right=S.sm,
            radius=T.radii.small,fill="surfaceContainer",
            ui.Text{id="icon",text="extension",family="Material Symbols Rounded",font_size=icon_size,width=icon_size,height=icon_size},
            ui.Column{id="copy",width=copy_width,gap=S.hairline,name,access}}
        if update then table.insert(line,update) end
        table.insert(line,enable)
        table.insert(column,ui.Row(line))
    end
    return ui.Scroll{id="plugin-scroll",grow=1,scroll=ui.bind("launcher.plugin.scroll"),ui.Column(column)}
end
