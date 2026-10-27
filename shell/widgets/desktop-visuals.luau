-- SPDX-License-Identifier: GPL-3.0-only
-- Shared palette-led primitives for wallpaper-level widget compositions.
local context=require("context")
local ui,kit=context.ui,context.kit
local sp,tx=kit.tokens.spacing,kit.tokens.text
local M={}

-- One visual vocabulary for wallpaper widgets.  The accent remains a palette
-- role so changing the wallpaper changes the widget family with it.
M.glass_fill=ui.alpha("surface",.62)
M.glass_border=ui.alpha("onSurface",.08)
M.text="onSurface"
M.secondary="onSurfaceVariant"
M.display_font="Outfit"
M.display_weight=200
M.body_font="IBM Plex Sans"
M.display_shadow=ui.alpha("shadow",.14)
M.radius=kit.tokens.radii.large

function M.glass(id,width,height,padding,children)
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
    local children={}
    for index,offset in ipairs({{-3,0},{-2,-2},{0,-3},{2,-2},{3,0},{2,2},{0,3},{-2,2}}) do
        table.insert(children,ui.Text{id=id.."-shadow-"..index,position="absolute",left=offset[1],top=offset[2],
            text=value,width=width,height=height,font_size=size,font_weight=M.display_weight,
            family=M.display_font,color=M.display_shadow})
    end
    props.id=id
    table.insert(children,ui.Text(props))
    return ui.Box{id=id.."-shadow-layer",width=width,height=height,fill="transparent",table.unpack(children)}
end

function M.shadow_text(id,value,width,height,size,color)
    local props={text=value,width=width,height=height,font_size=size,font_weight=400,
        family=M.body_font,color=color or M.text}
    local children={}
    for index,offset in ipairs({{-3,0},{-2,-2},{0,-3},{2,-2},{3,0},{2,2},{0,3},{-2,2}}) do
        table.insert(children,ui.Text{id=id.."-shadow-"..index,position="absolute",left=offset[1],top=offset[2],
            text=value,width=width,height=height,font_size=size,font_weight=400,family=M.body_font,color=M.display_shadow})
    end
    props.id=id
    table.insert(children,ui.Text(props))
    return ui.Box{id=id.."-shadow-layer",width=width,height=height,fill="transparent",table.unpack(children)}
end

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
