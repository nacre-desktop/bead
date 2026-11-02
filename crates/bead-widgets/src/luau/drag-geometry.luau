-- SPDX-License-Identifier: MIT
-- Rectangular collision and nearest-free placement helpers for host-supplied slots.
local M={}
local function overlaps(slot,others)
    for _,other in others do
        local o=other.slot
        if type(o)=="table" and slot.x<o.x+o.w and o.x<slot.x+slot.w and slot.y<o.y+o.h and o.y<slot.y+slot.h then
            return o
        end
    end
    return nil
end
local function desktop_landing(others,x,y,w,h,width,height,gap)
    local function place(px,py)
        return {x=math.floor(math.clamp(px,0,math.max(0,width-w))+.5),
            y=math.floor(math.clamp(py,0,math.max(0,height-h))+.5),w=w,h=h}
    end
    local slot=place(x,y)
    local hit=overlaps(slot,others)
    if not hit then return slot end
    gap=tonumber(gap) or 8
    local best,best_distance
    for _,o in others do
        local o=o.slot
        if type(o)=="table" then
            for _,candidate in {place(o.x-w-gap,y),place(o.x+o.w+gap,y),place(x,o.y-h-gap),place(x,o.y+o.h+gap)} do
                local distance=(candidate.x-x)^2+(candidate.y-y)^2
                if not overlaps(candidate,others) and (not best or distance<best_distance) then
                    best,best_distance=candidate,distance
                end
            end
        end
    end
    return best or slot
end
M.overlaps=overlaps
M.desktop_landing=desktop_landing
return M
