-- SPDX-License-Identifier: GPL-3.0-only
-- Store defaults for the Colors window. `nacre-shell/src/colors.rs` projects every `colors.*`
-- value from the daemon topics; page, query, vision and the fine-tune switch are view state.
return {stores={
    ["colors.page"]="studio",["colors.query"]="",["colors.vision"]="normal",["colors.fine"]=false,["colors.open"]="",
    ["colors.scroll"]=0,["colors.rail.scroll"]=0,["colors.win_w"]=1100,
    ["colors.view"]={},["colors.entries"]={},["colors.empty"]=false,
},actions={
    -- The Colors adapter authorizes and handles this host action (page, style, pick, apply...).
    ["colors.do"]=function(_) end,
},computed={}}
