-- SPDX-License-Identifier: GPL-3.0-only
-- Shared palette-led primitives for wallpaper-level widget compositions.
local context=require("context")
local ui,kit=context.ui,context.kit
local sp,tx=kit.tokens.spacing,kit.tokens.text
local M={}

-- One visual vocabulary for wallpaper widgets.  The accent remains a palette
-- role so changing the wallpaper changes the widget family with it.
M.glass_fill={10,22,20,158}
M.glass_border=ui.alpha("onSurface",.08)
M.text={238,246,243,255}
M.secondary="onSurfaceVariant"
M.display_font="Outfit"
M.display_weight=200
M.body_font="IBM Plex Sans"
M.radius=kit.tokens.radii.large

function M.glass(id,width,height,padding,children)
    local props={id=id,width=width,height=height,padding=padding or sp.lg,
        radius=M.radius,fill=M.glass_fill,border_width=1,border_color=M.glass_border}
    for _,child in children do table.insert(props,child) end
    return ui.Box(props)
end

function M.reference_glass(id,width,height,padding,children)
    local props={id=id,width=width,height=height,padding=padding or sp.lg,
        radius=M.radius,fill=M.glass_fill,border_width=1,border_color=M.glass_border}
    for _,child in children do table.insert(props,child) end
    return ui.Box(props)
end

function M.display(id,value,width,height,size,color,shadow)
    local props={text=value,width=width,height=height,font_size=size,
        font_weight=M.display_weight,family=M.display_font,color=color or M.text}
    if not shadow then
        props.id=id
        return ui.Text(props)
    end
    props.color=color or M.text
    props.text_shadow_blur=24
    props.text_shadow_y=2
    props.text_shadow_color=ui.alpha("shadow",.60)
    props.id=id
    return ui.Text(props)
end

function M.shadow_text(id,value,width,height,size,color)
    local props={text=value,width=width,height=height,font_size=size,font_weight=400,
        family=M.body_font,color=color or M.text,text_shadow_blur=24,text_shadow_y=2,
        text_shadow_color=ui.alpha("shadow",.60)}
    props.id=id
    return ui.Text(props)
end

function M.slot(instance,w,h)
    local slot=instance and instance.slot or {}
    return slot.w or w,slot.h or h
end

function M.ring(id,diameter,value,caption,detail)
    local fraction=math.clamp(tonumber(value) or 0,0,100)/100
    local reading=tostring(value or 0).."%"
    return ui.Box{id=id,width=diameter,height=diameter,fill="transparent",
        ui.Box{id="inner-disc",position="absolute",left=8,top=8,width=diameter-16,height=diameter-16,
            radius=(diameter-16)/2,fill={10,22,20,150}},
        ui.Arc{id="track",position="absolute",left=8,top=8,width=diameter-16,height=diameter-16,
            start_angle=-90,sweep_angle=360,stroke_width=16,color=ui.alpha("onSurface",.16),round_caps=true},
        ui.Arc{id="value",position="absolute",left=8,top=8,width=diameter-16,height=diameter-16,
            start_angle=-90,sweep_angle=ui.animate(id.."-sweep",fraction*360,.24,"out-cubic"),
            stroke_width=16,color="primary",round_caps=true},
        ui.Column{id="ring-copy",position="absolute",left=12,top=diameter*.28,width=diameter-24,height=diameter*.46,
            align="center",justify="center",gap=sp.xs,
            ui.Text{id="reading",text=reading,width="fill",height=82,font_size=64,font_weight=200,family=M.display_font,text_align="center",color=M.text},
            ui.Text{id="caption",text=caption or "",width="fill",height=22,font_size=tx.caption*4/3,text_align="center",color=M.secondary}},
        detail and ui.Text{id="detail",position="absolute",left=0,top=diameter+sp.xs,width=diameter,height=18,
            text=detail,font_size=tx.caption*4/3,text_align="center",color="onSurfaceVariant"} or nil}
end

return M
