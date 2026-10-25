-- SPDX-License-Identifier: GPL-3.0-only
-- Edit affordances and destination geometry belong to the shell's host composition.
local c=require("context")
local ui,kit=c.ui,c.kit
local sp=kit.tokens.spacing
local M={}
function M.seed(document,width,height)
    local reservations={}
    for _,item in document.hosts.top_bar.instances do
        local package=require("registry").catalogue[item.widget]
        local variant=package and package.variants.bar
        if variant and variant.reserve_width then
            local settings=table.clone(package.defaults)
            for key,value in item.settings or {} do settings[key]=value end
            table.insert(reservations,{iid=item.iid,width=variant.reserve_width(settings)})
        end
    end
    return {{widget_layout=true,document=document,initial=require("default")(width),rev=ui.resolve_for_check(ui.bind("widgets.rev")),
        geometry=ui.resolve_for_check(ui.bind("widgets.geometry")),reservations=reservations,canvas_width=width,canvas_height=height}}
end
function M.request(_document,extra)
    return ui.action("widgets.edit",{extra})
end
function M.decorate(document,item,node)
    local function quiet(child)
        local p=child.props or {}
        if p.on_drag then return end
        p.on_activate=nil
        for _,nested in child.children or {} do quiet(nested) end
    end
    quiet(node)
    local p=node.props or {}
    local props={id="widget-"..item.iid,height=p.height or c.header,width=p.width,shrink=0,padding=0,
        fill="transparent",hover_fill=ui.alpha("primary",.08),focus_on_pointer=false,activate_on_release=true,
        drag_payload=item.iid,on_drag=ui.action("widgets.drag"),on_activate=M.request(document,{op="select",iid=item.iid}),node,
        ui.Text{id="handle",position="absolute",left=0,top=0,width=sp.md,height=sp.md,text="drag_indicator",font_size=sp.md,family="Material Symbols Rounded",color="primary"},
        kit.IconButton{id="remove",position="absolute",right=0,top=0,width=sp.md,height=sp.md,padding=0,font_size=sp.md,text="close",focus_on_pointer=false,
            on_activate=M.request(document,{op="remove",iid=item.iid})}}
    if item.widget=="pill" then
        local start=0
        for _,child in item.children or {} do if child.slot.region=="centre" then start=math.max(start,child.slot.start+child.slot.span) end end
        props.on_drop=M.request(document,{op="move",host=item.iid,region="centre",start=start})
    end
    return ui.Button(props)
end
function M.targets(document,width,height,bar)
    local nodes={}
    local header=c.header
    if bar then
        for tile=0,document.hosts.top_bar.tiles do
            table.insert(nodes,ui.Box{id="tile-"..tile,position="absolute",left=tile*width/document.hosts.top_bar.tiles,top=0,
                width=kit.tokens.spacing.hairline,height=header,fill=ui.alpha("primary",.10)})
        end
    end
    for index,region in {"left","centre","right"} do
        table.insert(nodes,ui.Box{id="drop-"..region,position="absolute",left=(index-1)*width/3,top=0,width=width/3,height=header,
            on_drop=M.request(document,{op="move",host="top_bar",region=region,start=0,point=true,host_width=width})})
    end
    table.insert(nodes,1,ui.Box{id="drop-canvas",position="absolute",left=0,top=header,width=width,height=math.max(1,height-header),
        on_drop=M.request(document,{op="move",host="canvas",point=true,x=sp.huge,y=sp.huge*3,w=sp.huge*6,h=sp.huge*2,snap=sp.huge})})
    return nodes
end
function M.ghost(document,catalogue,render)
    local source=ui.resolve_for_check(ui.bind("widgets.drag.source"))
    if source=="" then return nil end
    local function find(items)
        for _,item in items do
            if item.iid==source then return item end
            local child=item.children and find(item.children)
            if child then return child end
        end
    end
    local item
    for _,host in document.hosts do item=find(host.instances) if item then break end end
    local package=catalogue[item and item.widget or source:gsub("^catalogue:","")]
    if not package then return nil end
    item=item or {iid="preview",widget=package.id,host="top_bar",slot={region="left",start=0,span=package.variants.bar.preferred_span},settings=package.defaults}
    local node=render(item,package.variants.bar)
    local function quiet(child)
        for key in child.props do
            if type(key)=="string" and (key:sub(1,3)=="on_" or key=="drag_payload") then child.props[key]=nil end
        end
        for _,nested in child.children or {} do quiet(nested) end
    end
    quiet(node)
    return ui.Box{id="widget-ghost",position="absolute",left=ui.bind("widgets.drag.x"),top=ui.bind("widgets.drag.y"),visible=ui.bind("widgets.drag.active"),
        width=sp.huge*6,height=sp.huge*2,radius=kit.tokens.radii.small,fill="surfaceContainerHighest",padding=sp.sm,node}
end
return M
