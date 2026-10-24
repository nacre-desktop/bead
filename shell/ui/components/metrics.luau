-- SPDX-License-Identifier: GPL-3.0-only
-- QML launcher detail geometry, expressed with the shared theme scale.
local T=require("bead.kit").tokens
local S=T.spacing
local M={}
M.quick_width=S.xl*13
M.quick_gap=S.xl
M.quick_top=S.xxl*3+S.md
M.quick_height=S.huge*13-S.compact
M.quick_radius=T.radii.small+S.compact/2
M.quick_padding=S.md+S.hairline/2
M.button_radius=T.radii.small+S.hairline/2
M.row_height=S.huge+S.xl
M.reason_advance=.5
M.disabled_opacity=.5
M.hover_opacity=.08
M.pressed_opacity=.12
M.quick_order={"actions.controls.dnd","actions.controls.night-light","actions.controls.stay-awake"}
M.quick_icons={
    ["actions.controls.dnd"]="notifications_off",
    ["actions.controls.night-light"]="bedtime",
    ["actions.controls.stay-awake"]="coffee",
}
M.width=function(viewport) return math.min(1100,viewport*.9) end
M.show=function(viewport,query) return M.width(viewport)>=T.icons.hero*30-S.xl-S.hairline and not (query and query:match("^%s*=")) end
M.content_width=function(viewport,query)
    return M.width(viewport)-12-20-152-18-6-(M.show(viewport,query) and M.quick_width+M.quick_gap or 0)
end
return M
