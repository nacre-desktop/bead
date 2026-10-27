-- SPDX-License-Identifier: GPL-3.0-only
-- Shared palette-led primitives for wallpaper-level widget compositions.
local context=require("context")
local ui,kit=context.ui,context.kit
local sp,tx=kit.tokens.spacing,kit.tokens.text
local M={}

function M.slot(instance,w,h)
    local slot=instance and instance.slot or {}
    return slot.w or w,slot.h or h
end

function M.ring(id,diameter,value,caption,detail)
    local fraction=math.clamp(tonumber(value) or 0,0,100)/100
    local reading=tostring(value or 0).."%"
    return ui.Box{id=id,width=diameter,height=diameter,fill="transparent",
        ui.Arc{id="track",position="absolute",left=4,top=4,width=diameter-8,height=diameter-8,
            start_angle=-90,sweep_angle=360,stroke_width=8,color=ui.alpha("onSurface",.16),round_caps=true},
        ui.Arc{id="value",position="absolute",left=4,top=4,width=diameter-8,height=diameter-8,
            start_angle=-90,sweep_angle=ui.animate(id.."-sweep",fraction*360,.24,"out-cubic"),
            stroke_width=8,color="primary",round_caps=true},
        ui.Column{id="ring-copy",position="absolute",left=0,top=diameter*.32,width=diameter,height=diameter*.4,
            align="center",justify="center",gap=sp.xs,
            ui.Text{id="reading",text=reading,width="fill",height=26,font_size=tx.title*4/3,text_align="center",color="primary"},
            ui.Text{id="caption",text=caption or "",width="fill",height=18,font_size=tx.caption*4/3,text_align="center",color="onSurfaceVariant"}},
        detail and ui.Text{id="detail",position="absolute",left=0,top=diameter+sp.xs,width=diameter,height=18,
            text=detail,font_size=tx.caption*4/3,text_align="center",color="onSurfaceVariant"} or nil}
end

return M
