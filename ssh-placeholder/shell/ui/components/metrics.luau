-- SPDX-License-Identifier: GPL-3.0-only
-- QML launcher detail geometry, expressed with the shared theme scale.
local T=require("bead.kit").tokens
local ui=require("bead.ui")
local S=T.spacing
local M={}
function M.hovered(id)
    local key=id:gsub(".",function(c) return string.format("%02x",string.byte(c)) end)
    return string.find(ui.resolve_for_check(ui.bind("bead.hovered")),".k"..key..".",1,true)~=nil
end
-- Keep launcher image sizes in UI settings; the shell prewarmer reads the
-- matching offscreen warm scene rather than duplicating layout constants.
M.grid_icon=36
M.result_icon=30
M.grid_cell_width=110
M.grid_row_height=76.2
M.grid_gap_x=6
M.grid_gap_y=5
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
M.show=function(viewport,allowed) return M.width(viewport)>=T.icons.hero*30-S.xl-S.hairline and allowed~=false end
M.content_width=function(viewport,allowed)
    return M.width(viewport)-12-20-152-18-6-(M.show(viewport,allowed) and M.quick_width+M.quick_gap or 0)
end
return M
