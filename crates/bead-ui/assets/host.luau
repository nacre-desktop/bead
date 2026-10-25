-- SPDX-License-Identifier: MIT OR Apache-2.0
-- A host owns placement, never package data sources or shell routes.
return function(ui)
    local function integer(n, lo, hi, label)
        assert(type(n)=="number" and n%1==0 and n>=lo and n<=hi, "Host: invalid "..label)
        return n
    end
    local function merge(a,b)
        local out=table.clone(a or {})
        for k,v in b or {} do out[k]=v end
        return out
    end
    return function(p)
        if p.shape=="canvas" then
            local layout=ui.resolve_for_check(p.layout)
            assert(type(layout)=="table","Host: canvas layout must be an object")
            local function dimension(n,label)
                assert(type(n)=="number" and n==n and n>0 and n<=20000,"Host: invalid canvas "..label)
                return n
            end
            local width=dimension(layout.width,"width")
            local height=dimension(layout.height,"height")
            assert(type(layout.instances)=="table","Host: canvas instances must be a list")
            local children={}
            for _,instance in layout.instances do
                local slot=instance.slot
                assert(type(slot)=="table","Host: canvas slot must be an object")
                local x,y=slot.x,slot.y
                local w,h=slot.w,slot.h
                assert(type(x)=="number" and x==x and x>=0 and x<=20000
                    and type(y)=="number" and y==y and y>=0 and y<=20000
                    and type(w)=="number" and w==w and w>0 and w<=20000
                    and type(h)=="number" and h==h and h>0 and h<=20000
                    and x+w<=width and y+h<=height,"Host: canvas slot exceeds bounds")
                local package=p.catalogue[instance.widget]
                local variant=package and (package.variants.canvas or package.variants.card or package.variants.bar)
                if variant then
                    local node=p.render(instance,variant)
                    if node then
                        node=ui.Box(merge({id=instance.iid,position="absolute",left=x,top=y,width=w,height=h}, {node}))
                        table.insert(children,node)
                    end
                end
            end
            local props={}
            for k,v in p do
                if k~="shape" and k~="layout" and k~="catalogue" and k~="render" and k~="flatten" then props[k]=v end
            end
            props.width=width; props.height=height
            for _,node in children do table.insert(props,node) end
            return ui.Box(props)
        end
        assert(p.shape=="strip-h", "Host: only strip-h is supported")
        local layout=ui.resolve_for_check(p.layout)
        assert(type(layout)=="table","Host: layout must be a list")
        local grid=p.grid or {}
        local derived
        if grid.tile_size then
            assert(type(grid.tile_size)=="number" and grid.tile_size>0,"Host: invalid tile_size")
            local width=ui.resolve_for_check(grid.width or p.width)
            assert(type(width)=="number" and width>0,"Host: tile_size requires a numeric width")
            derived=math.max(1,math.floor(width/grid.tile_size+.5))
        end
        local tiles=integer(grid.tiles or derived or 60,1,4096,"tiles")
        local gap=grid.gap or 0
        assert(type(gap)=="number" and gap>=0 and gap<=256,"Host: invalid gap")
        local content=grid.mode=="content"
        assert(grid.mode==nil or grid.mode=="tiles" or content,"Host: invalid grid mode")
        local regions=grid.regions or {left={start=0,span=tiles},centre={start=0,span=tiles},right={start=0,span=tiles}}
        local groups={left={},centre={},right={}}
        local occupied={left={},centre={},right={}}
        for _,instance in layout do
            local slot=instance.slot
            local region=slot.region
            assert(groups[region] and regions[region],"Host: invalid region")
            local def=p.catalogue[instance.widget]
            -- Unknown packages remain in the document, but do not paint outside edit mode.
            if def then
                local variant=def.variants.bar
                assert(variant,"Host: missing bar variant")
                local span=integer(slot.span or variant.preferred_span,1,4096,"span")
                local compact=def.variants.compact
                local chosen=variant
                if compact and span>=compact.min_span and span<=compact.max_span
                    and (span<variant.preferred_span or span>variant.max_span) then
                    chosen=compact
                end
                integer(span,chosen.min_span,chosen.max_span,"variant span")
                local r=regions[region]
                local capacity=integer(r.span or tiles,1,tiles,"region span")
                local start=integer(slot.start or 0,0,capacity-1,"start")
                assert(start+span<=capacity,"Host: slot exceeds region")
                for tile=start,start+span-1 do
                    assert(not occupied[region][tile],"Host: overlapping slots")
                    occupied[region][tile]=true
                end
                local node=p.render(instance,chosen)
                if node then
                    if not content then
                        local column=region=="right" and capacity-start-span+1 or start+1
                        local props={id=instance.iid,height="100%",column_start=column,column_span=span,align="center",justify="center"}
                        node=ui.Row(merge(props,{node}))
                    end
                    table.insert(groups[region],{start=start,node=node})
                end
            end
        end
        local children={}
        for _,name in {"left","centre","right"} do
            local r=regions[name]
            if r then
                local start=integer(r.start or 0,0,tiles-1,"region start")
                local span=integer(r.span or tiles,1,tiles-start,"region span")
                table.sort(groups[name],function(a,b) return a.start<b.start end)
                local nodes={}
                for _,node in r.before or {} do table.insert(nodes,node) end
                for _,entry in groups[name] do table.insert(nodes,entry.node) end
                for _,node in r.after or {} do table.insert(nodes,node) end
                if r.inline and content then
                    for _,node in nodes do table.insert(children,node) end
                else
                    local style=merge({id=name,height="100%",gap=gap,align="center",shrink=0},r.props)
                    if not content then
                        style.position="absolute"
                        style.left=tostring(start*100/tiles).."%"
                        style.width=tostring(span*100/tiles).."%"
                        style.top=0
                        style.columns=span
                        style.row_height=ui.resolve_for_check(p.height) or 50
                    end
                    for _,node in nodes do table.insert(style,node) end
                    table.insert(children,(content and ui.Row or ui.Grid)(style))
                end
            end
        end
        local props={}
        for k,v in p do
            if k~="shape" and k~="grid" and k~="layout" and k~="catalogue" and k~="render" and k~="flatten" then props[k]=v end
        end
        for _,node in children do table.insert(props,node) end
        return (p.flatten and ui.Group or ui.Row)(props)
    end
end
