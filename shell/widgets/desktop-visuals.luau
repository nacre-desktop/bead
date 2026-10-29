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
M.radius=kit.tokens.radii.large

function M.glass(id,width,height,padding,children)
    local props={id=id,width=width,height=height,padding=padding or sp.lg,
        radius=M.radius,fill=M.glass_fill,border_width=1,border_color=M.glass_border}
    for _,child in children do table.insert(props,child) end
    return ui.Box(props)
end

function M.reference_glass(id,width,height,padding,children)
    local props={id=id,width=width,height=height,padding=padding or sp.lg,
        radius=M.radius,fill=ui.alpha("surfaceContainer",.72),border_width=1,border_color=ui.alpha("outlineVariant",.35)}
    for _,child in children do table.insert(props,child) end
    return ui.Box(props)
end

-- B0193 adaptive contrast.  The wallpaper's luminance grid (bead.desktop.wallpaper.luma =
-- {{cols,rows,on_surface,surface,cells}}, relative luminance 0..1, computed once per
-- wallpaper by the host) tells a text widget which palette role reads best under its slot.
-- Roles only, never literals: ink is onSurface or surface, whichever contrasts more with the
-- wallpaper under the widget; when neither reaches `minimum` a translucent rounded scrim in
-- the opposite role goes behind the text.  The canvas entry supplies the layout size.
local layout={w=0,h=0}
local SCRIM_ALPHA=.55
function M.set_canvas(width,height) layout.w,layout.h=width,height end
local function contrast(a,b) return (math.max(a,b)+.05)/(math.min(a,b)+.05) end
local DEFAULT_TONE={ink=M.text,secondary=M.secondary,shadow=ui.alpha("shadow",.50)}
-- Mean luminance of the cells under the slot, plus the two role luminances.
function M.wallpaper_luma(instance)
    local slot=instance and instance.slot
    local record=ui.resolve_for_check(ui.bind("bead.desktop.wallpaper.luma"))
    local grid=type(record)=="table" and record[1] or nil
    if not slot or not slot.x or not grid or layout.w<=0 or layout.h<=0 then return nil end
    local cols,rows,cells=grid.cols,grid.rows,grid.cells
    if not cols or not rows or not cells or cols<1 or rows<1 or #cells<cols*rows then return nil end
    local x0=math.clamp(math.floor(slot.x/layout.w*cols),0,cols-1)
    local x1=math.clamp(math.ceil((slot.x+slot.w)/layout.w*cols)-1,x0,cols-1)
    local y0=math.clamp(math.floor(slot.y/layout.h*rows),0,rows-1)
    local y1=math.clamp(math.ceil((slot.y+slot.h)/layout.h*rows)-1,y0,rows-1)
    local sum,count=0,0
    for row=y0,y1 do
        for col=x0,x1 do
            sum+=cells[row*cols+col+1]
            count+=1
        end
    end
    return sum/count,grid.on_surface,grid.surface
end
-- Pure choice, kept separate so a test can drive it with numbers.
function M.pick_tone(wall,on_surface,surface,minimum)
    local ink,ink_luma,backing,backing_luma="onSurface",on_surface,"surface",surface
    if contrast(surface,wall)>contrast(on_surface,wall) then
        ink,ink_luma,backing,backing_luma="surface",surface,"onSurface",on_surface
    end
    local tone={adapted=true,ink=ink,secondary=ink=="onSurface" and "onSurfaceVariant" or ui.alpha("surface",.8),
        shadow=ink=="onSurface" and ui.alpha("shadow",.50) or ui.alpha("onSurface",.35)}
    if contrast(ink_luma,wall)<minimum then
        local behind=wall*(1-SCRIM_ALPHA)+backing_luma*SCRIM_ALPHA
        if contrast(ink_luma,behind)>contrast(ink_luma,wall) then
            tone.scrim=ui.alpha(backing,SCRIM_ALPHA)
            tone.shadow=nil
        end
    end
    return tone
end
function M.tone(instance,minimum)
    local iid=instance and instance.iid
    if type(iid)=="string" and string.sub(iid,1,8)=="preview-" then return DEFAULT_TONE end
    local wall,on_surface,surface=M.wallpaper_luma(instance)
    if not wall then return DEFAULT_TONE end
    return M.pick_tone(wall,on_surface,surface,minimum or 4.5)
end
-- Box props that put the scrim behind a widget's content.
function M.scrim_props(tone)
    if not tone.scrim then return {fill="transparent"} end
    return {fill=tone.scrim,radius=M.radius}
end

-- `shadow`: true for the default glow, a colour for an adaptive one, nil/false for none.
local function shadow_colour(shadow)
    if shadow==true then return ui.alpha("shadow",.50) end
    return shadow
end

function M.display(id,value,width,height,size,color,shadow)
    local props={text=value,width=width,height=height,font_size=size,
        font_weight=M.display_weight,family=M.display_font,color=color or M.text}
    if not shadow then
        props.id=id
        return ui.Text(props)
    end
    props.text_shadow_blur=16
    props.text_shadow_y=2
    props.text_shadow_color=shadow_colour(shadow)
    props.id=id
    return ui.Text(props)
end

function M.shadow_text(id,value,width,height,size,color,shadow)
    local props={text=value,width=width,height=height,font_size=size,font_weight=400,
        family=M.body_font,color=color or M.text}
    if shadow~=false then
        props.text_shadow_blur=16
        props.text_shadow_y=2
        props.text_shadow_color=shadow_colour(shadow==nil and true or shadow)
    end
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
            radius=(diameter-16)/2,fill=ui.alpha("surfaceContainer",.88)},
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
