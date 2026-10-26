-- SPDX-License-Identifier: GPL-3.0-only
-- Small best-effort logical-size measurement for passive widget previews.
local context=require("context")
local ui,kit=context.ui,context.kit
local M={}

local function number(value)
    if value~=nil then value=ui.resolve_for_check(value) end
    return type(value)=="number" and value or nil
end

local function kind(node)
    return type(node.kind)=="string" and string.lower(node.kind) or ""
end

local function flow_children(node,depth,out)
    if depth>24 then return end
    for _,child in node.children or {} do
        if kind(child)=="group" then
            flow_children(child,depth+1,out)
        elseif (child.props or {}).position~="absolute" then
            table.insert(out,child)
        end
    end
end

function M.width(node,depth)
    depth=depth or 0
    if type(node)~="table" or depth>24 then return 0 end
    local props=node.props or {}
    local node_kind=kind(node)
    -- Layout flattens Group children into their parent, so Group styling does
    -- not constrain their intrinsic width or consume a parent flex gap.
    if node_kind=="group" then
        local children={}
        flow_children(node,depth,children)
        local width=0
        for _,child in children do width+=M.width(child,depth+1) end
        return width
    end
    if props.visible~=nil and ui.resolve_for_check(props.visible)==false then return 0 end
    local explicit=number(props.width)
    if explicit and explicit>=0 then return explicit end
    if node_kind=="text" then
        local value=ui.resolve_for_check(props.text or "") or ""
        return ui.text_width(tostring(value),props.family or "IBM Plex Sans",
            number(props.font_size) or kit.tokens.text.body*4/3)
    elseif node_kind=="image" then
        return number(props.height) or 0
    end
    local widths,maximum={},0
    local children={}
    flow_children(node,depth,children)
    for _,child in children do
        local width=M.width(child,depth+1)
        if width>0 then table.insert(widths,width); maximum=math.max(maximum,width) end
    end
    local padding=number(props.padding) or 0
    local left=number(props.padding_left) or padding
    local right=number(props.padding_right) or padding
    local content
    if node_kind=="column" then
        content=maximum
    else
        local gap=number(props.gap_x) or number(props.gap) or 0
        content=0
        for _,width in widths do content+=width end
        content+=math.max(0,#widths-1)*gap
    end
    if node_kind=="button" or node_kind=="textinput" then
        if props.padding==nil and props.padding_left==nil then left=kit.tokens.spacing.sm end
        if props.padding==nil and props.padding_right==nil then right=kit.tokens.spacing.sm end
    end
    return content+left+right
end

return M
