-- SPDX-License-Identifier: MIT
-- Canonical ordered slot packing. The host supplies regions and item-specific sizing/containment.
return function(options)
    local rank=options.region_rank
    local M={}
    function M.copy(value)
        if type(value)~="table" then return value end
        local result={}
        for key,item in value do result[key]=M.copy(item) end
        return result
    end
    function M.pack(list,top)
        local order={}
        for index,item in list do
            if type(item)=="table" and type(item.slot)=="table" then
                if not rank[item.slot.region] then item.slot.region=options.default_region end
                table.insert(order,{item=item,index=index,start=tonumber(item.slot.start) or 0})
            end
        end
        table.sort(order,function(a,b)
            local ar,br=rank[a.item.slot.region],rank[b.item.slot.region]
            if ar~=br then return ar<br end
            if a.start~=b.start then return a.start<b.start end
            return a.index<b.index
        end)
        local cursor={}
        for region in rank do cursor[region]=0 end
        table.clear(list)
        for _,entry in order do
            local item,region=entry.item,entry.item.slot.region
            item.slot.start=cursor[region]
            item.slot.span=options.span(item)
            item.slot.gap=nil
            local offset=tonumber(item.slot.offset)
            item.slot.offset=top and offset and offset==offset and math.abs(offset)<20000 and math.round(offset) or nil
            cursor[region]+=item.slot.span
            if not options.keep_children(item) then item.children=nil end
            if item.children then M.pack(item.children,false) end
            table.insert(list,item)
        end
    end
    return M
end
