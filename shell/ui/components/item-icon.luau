-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local glyphs=require("./glyphs")
return function(item,size)
    -- Desktop entries may use the same name as a symbol; preserve their theme icon.
    if item.kind~="app" and glyphs[item.icon] then
        return ui.Text{id="icon",text=item.icon,family="Material Symbols Rounded",font_size=size,
            width=size,height=size,shrink=0,color="onSurface",aria_label=item.name}
    end
    return ui.Image{id="icon",icon=item.icon,width=size,height=size,aria_label=item.name}
end
