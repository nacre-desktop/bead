-- SPDX-License-Identifier: GPL-3.0-only
-- Offscreen UI settings source for the launcher's shared image prewarmer.
local ui=require("bead.ui")
local M=require("./components/metrics")
return ui.Row{
    ui.Image{icon="application-x-executable",width=M.grid_icon,height=M.grid_icon},
    ui.Image{icon="application-x-executable",width=M.result_icon,height=M.result_icon},
}
