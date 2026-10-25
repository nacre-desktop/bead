-- SPDX-License-Identifier: GPL-3.0-only
-- Shell-owned catalogue, placement and schema-generated settings.
local context=require("../context")
local ui=context.ui
local kit=context.kit
local registry=require("../registry")
local sp=kit.tokens.spacing
local tx=kit.tokens.text
local draft=ui.resolve_for_check(ui.bind("widgets.draft"))
local selected=ui.resolve_for_check(ui.bind("widgets.selected"))
local saved=ui.resolve_for_check(ui.bind("widgets.layout"))
local document=(draft and draft[1]) or (saved and saved[1])
local function viewport_width()
    return ui.resolve_for_check(ui.bind("bead.viewport.width")) or 1920
end
local function default_document()
    return registry.default(viewport_width())
end
document=document or default_document()
local render_current=registry.renderer(document)
local function edit(op,extra)
    local request={op=op}
    for key,value in extra or {} do request[key]=value end
    return ui.action("widgets.edit",{request})
end
local function text(id,value,height,size,color)
    return ui.Text{id=id,text=value or "",height=height or 20,font_size=size or tx.body*4/3,color=color or "onSurface"}
end
local function button(id,label,action,width)
    local node=kit.Button{id=id,text=label,width=width,height=sp.huge+sp.xs,shrink=0,padding=sp.xs,
        focus_on_pointer=false,on_activate=action}
    local request=action and action.argument and action.argument[1]
    if request and (request.op=="move" or request.op=="remove" or request.op=="add") then node.props.on_drop=action end
    if request and (request.op=="select" or request.op=="add") then
        node.props.drag_payload=request.iid or {{widget=request.widget}}
        node.props.on_drag=ui.action("widgets.drag")
        node.props.activate_on_release=true
    end
    return node
end
if ui.resolve_for_check(ui.bind("widgets.context")) then
    local items={button("edit","Edit widgets",edit("begin"),"fill")}
    if selected~="" then table.insert(items,button("settings","Widget settings",edit("page",{page="settings"}),"fill")) end
    return kit.Panel{edge="top",id="widget-menu",width=sp.huge*10,height=sp.huge*(#items+2),metadata=require("../editing").seed(document,viewport_width(),ui.resolve_for_check(ui.bind("bead.viewport.height"))),
        open=ui.bind("panel.open"),padding=sp.sm,ui.Column{width="fill",gap=sp.xs,table.unpack(items)}}
end
local function find(items,iid)
    for _,item in items or {} do
        if item.iid==iid then return item end
        local nested=find(item.children,iid); if nested then return nested end
    end
end
local function sorted_keys(values)
    local keys={}
    for key in values do table.insert(keys,key) end
    table.sort(keys)
    return keys
end
local function wire_control(node,dummy,item,key)
    if type(node)~="table" then return end
    local props=node.props or node
    local action=props.on_activate
    if type(action)=="table" and action.name==dummy then
        props.on_activate=edit("setting",{iid=item.iid,key=key,value=action.argument})
        props.focus_on_pointer=false
        if props.pointer_arg~=nil then props.pointer_arg=edit("setting",{iid=item.iid,key=key,value=props.pointer_arg}).argument end
    end
    for _,child in node.children or {} do wire_control(child,dummy,item,key) end
end
local function add_flat(items,out,depth)
    for _,item in items or {} do
        table.insert(out,{item=item,depth=depth})
        add_flat(item.children,out,depth+1)
    end
end
local placed={}; add_flat(document.hosts.top_bar.instances,placed,0)
if document.hosts.canvas then add_flat(document.hosts.canvas.instances,placed,0) end
local current=find(document.hosts.top_bar.instances,selected) or (document.hosts.canvas and find(document.hosts.canvas.instances,selected))
local function packed_start(list,region)
    local start=0
    for _,item in list do
        if item.slot.region==region then start=math.max(start,item.slot.start+item.slot.span) end
    end
    return start
end
local function safe_preview(node)
    if type(node)~="table" then return end
    local props=node.props or node
    for key in props do
        if type(key)=="string" and (string.sub(key,1,3)=="on_" or key=="focus_on_pointer") then props[key]=nil end
    end
    if type(props.render)=="function" then
        local render=props.render
        props.render=function(...) local child=render(...) safe_preview(child) return child end
    end
    for _,child in node.children or {} do safe_preview(child) end
end
local rows={id="placed-list",width="fill",gap=sp.xs,shrink=0}
for _,entry in placed do
    local item=entry.item
    local row={id="instance-"..item.iid,width="fill",height=sp.huge+sp.sm,align="center",gap=sp.xs,
        padding_left=sp.sm+entry.depth*sp.lg,radius=kit.tokens.radii.small,
        fill=item.iid==selected and "secondaryContainer" or "surfaceContainer"}
    row[1]=button("select",item.widget,edit("select",{iid=item.iid}),140)
    local span=item.slot.span or 1
    local label=item.host=="canvas" and ("Desktop · "..item.slot.x..", "..item.slot.y) or (item.slot.region.." · tile "..tostring(item.slot.start+1).." · span "..span)
    row[2]=text("slot",label,20,tx.caption,"onSurfaceVariant")
    row[3]=button("remove","×",edit("remove",{iid=item.iid}),40)
    for _,region in {"left","centre","right"} do
        local bounds=document.hosts.top_bar.regions[region]
        local start=packed_start(document.hosts.top_bar.instances,region)
        if bounds and start+span<=bounds.span and (region~=item.slot.region or start~=item.slot.start) then
            table.insert(row,button("to-"..region,region,edit("move",{iid=item.iid,host="top_bar",region=region,start=start,span=span}),72))
        end
    end
    local pill=find(document.hosts.top_bar.instances,"status-pill")
    if pill and item.iid~="status-pill" then
        local start=packed_start(pill.children or {},"centre")
        if start+span<=document.hosts.top_bar.tiles then
            table.insert(row,button("to-pill","Pill",edit("move",{iid=item.iid,host="status-pill",region="centre",start=start,span=span}),64))
        end
    end
    local package=registry.catalogue[item.widget]
    if package then
        local preview=render_current(item,package.variants.bar)
        safe_preview(preview)
        table.insert(row,ui.Box{id="live-preview",width=sp.huge+sp.lg,height=sp.huge,fill="surface",preview})
    end
    table.insert(rows,ui.Row(row))
end
if #placed==0 then table.insert(rows,text("empty","No widgets are placed yet.",24,tx.body,"onSurfaceVariant")) end
local function setting_row(item,key,schema,value)
    local label=key:gsub("_"," ")
    if schema.type=="toggle" then
        return kit.ToggleRow{id="field-"..key,label=label,checked=value==true,enabled=true,
            on_activate=edit("setting",{iid=item.iid,key=key,value=not value})}
    elseif schema.type=="choice" then
        local options={}
        for _,choice in schema.options do
            table.insert(options,{id=choice,label=tostring(choice),selected=choice==value})
        end
        local control
        if #options<=3 then
            control=kit.SegmentedControl{id="control-"..key,value=value,on_change="widgets.choice",items=options}
        else
            control=kit.SelectRow{id="control-"..key,label=label,text=tostring(value or ""),options=options,
                enabled=true,open=ui.resolve_for_check(ui.bind("widgets.select"))=="choice:"..key,
                on_toggle=edit("menu",{key=key}),width=sp.huge*9,on_change="widgets.choice"}
        end
        wire_control(control,"widgets.choice",item,key)
        if #options<=3 then return ui.Column{id="choice-"..key,width="fill",shrink=0,gap=sp.xs,text("label",label),control} end
        return control
    elseif schema.type=="slider" then
        local control=kit.Slider{id="slider-"..key,label=label,value=value or schema.min,min=schema.min,max=schema.max,
            step=schema.step or (schema.max-schema.min)/20,width=sp.huge*9,on_change="widgets.choice"}
        wire_control(control,"widgets.choice",item,key)
        return control
    elseif schema.type=="number" then
        local step=schema.step or ((schema.max-schema.min)/10)
        return ui.Row{id="number-"..key,width="fill",height=sp.huge+sp.sm,align="center",gap=sp.xs,
            text("label",label,22,tx.body),button("minus","−",edit("setting",{iid=item.iid,key=key,value=math.max(schema.min,(value or schema.min)-step)}),40),
            text("value",tostring(value or schema.min),22,tx.body),button("plus","+",edit("setting",{iid=item.iid,key=key,value=math.min(schema.max,(value or schema.min)+step)}),40)}
    elseif schema.type=="text" or schema.type=="color" or schema.type=="icon" then
        local editing=ui.resolve_for_check(ui.bind("widgets.select"))==key
        return kit.TextRow{id="field-"..key,label=label,text=tostring(value or ""),editing=editing,enabled=true,
            field=ui.bind("widgets.field"),on_edit=edit("field",{iid=item.iid,key=key,value=value or ""}),
            on_save=edit("setting",{iid=item.iid,key=key,from_field=true}),on_cancel=edit("select",{iid=item.iid})}
    end
    return nil
end
local form={id="settings-fields",width="fill",gap=sp.sm,shrink=0}
if current then
    local package=registry.catalogue[current.widget]
    if package then
        for _,key in sorted_keys(package.settings) do
            local schema=package.settings[key]
            local value=(current.settings or {})[key]
            if value==nil then value=package.defaults[key] end
            table.insert(form,setting_row(current,key,schema,value))
        end
    end

end
local global_trigger=document.dropdown_trigger or "click"
local catalogue={id="catalogue",width="fill",gap=sp.sm,shrink=0}
for _,id in sorted_keys(registry.catalogue) do
    local package=registry.catalogue[id]
    local variant=package.variants.bar
    local span=variant.preferred_span
    local destination
    for _,region in {"left","centre","right"} do
        local bounds=document.hosts.top_bar.regions[region]
        if bounds then
            for start=0,bounds.span-span do
                local free=true
                for _,placed in document.hosts.top_bar.instances do
                    if placed.slot.region==region and start<placed.slot.start+placed.slot.span and start+span>placed.slot.start then free=false break end
                end
                if free then destination={widget=id,host="top_bar",region=region,start=start,span=span} break end
            end
        end
        if destination then break end
    end
    local instance={iid="catalogue-"..id,widget=id,host="top_bar",slot={region="left",start=0,span=span},settings=table.clone(package.defaults)}
    local preview=variant.render({instance=instance,catalogue=registry.catalogue,render=function(child,child_variant)
        local child_package=registry.catalogue[child.widget]
        return (child_variant or child_package.variants.bar).render({instance=child,catalogue=registry.catalogue,render=function() end},child.settings or child_package.defaults,{})
    end},package.defaults,{})
    safe_preview(preview)
    table.insert(catalogue,ui.Column{id="package-"..id,width="fill",shrink=0,gap=sp.xs,padding=sp.sm,
        radius=kit.tokens.radii.small,fill="surfaceContainer",
        ui.Row{id="package-heading",width="fill",align="center",gap=sp.sm,
            text("name",package.title,24,tx.title),text("category",package.category,20,tx.caption,"onSurfaceVariant"),
            button("add",destination and "Add" or "Full",destination and edit("add",destination) or nil,68)},
        ui.Box{id="preview",width="fill",height=sp.huge+sp.lg,fill="surface",preview}})
end
local drag_source=ui.resolve_for_check(ui.bind("widgets.drag.source"))
local dragging=find(document.hosts.top_bar.instances,drag_source) or (document.hosts.canvas and find(document.hosts.canvas.instances,drag_source))
local drag_package=registry.catalogue[dragging and dragging.widget or drag_source:gsub("^catalogue:","")]
local ghost
if drag_package then
    local item=dragging or {iid="drag-preview",widget=drag_package.id,host="top_bar",slot={region="left",start=0,span=drag_package.variants.bar.preferred_span},settings=drag_package.defaults}
    local preview=render_current(item,drag_package.variants.bar)
    safe_preview(preview)
    ghost=ui.Box{id="drag-preview",position="absolute",left=ui.bind("widgets.drag.x"),top=ui.bind("widgets.drag.y"),
        width=sp.huge*6,height=sp.huge*2,visible=ui.bind("widgets.drag.active"),padding=sp.sm,
        fill="surfaceContainerHighest",radius=kit.tokens.radii.medium,preview}
end
local page=ui.resolve_for_check(ui.bind("widgets.page"))
local trash=button("trash","Trash",nil)
trash.props.on_drop=edit("remove",{})
local content={id="editor-content",width="fill",gap=sp.lg}
local viewport_width=viewport_width()
local viewport_height=ui.resolve_for_check(ui.bind("bead.viewport.height")) or sp.huge*40
-- SurfaceOrigin is the output coordinate of this source's layout origin. Drop
-- events are source-relative; the edit requests carry the matching offset back.
local surface_x=ui.resolve_for_check(ui.bind("bead.surface.x")) or 0
local surface_y=ui.resolve_for_check(ui.bind("bead.surface.y")) or 0
local panel_width,panel_height=sp.huge*28,sp.huge*21
local drop_zones={}
local function add_outside_drop(id,rect,request)
    local x,y,w,h=rect[1],rect[2],rect[3],rect[4]
    x=math.max(0,x); y=math.max(0,y)
    local right,bottom=math.min(viewport_width,x+w),math.min(viewport_height,y+h)
    if right<=x or bottom<=y then return end
    local px,py=surface_x,surface_y
    local pr,pb=px+panel_width,py+panel_height
    local ix,iy,ir,ib=math.max(x,px),math.max(y,py),math.min(right,pr),math.min(bottom,pb)
    local pieces={}
    if ir<=ix or ib<=iy then pieces[1]={x,y,right-x,bottom-y}
    else
        if iy>y then table.insert(pieces,{x,y,right-x,iy-y}) end
        if ib<bottom then table.insert(pieces,{x,ib,right-x,bottom-ib}) end
        if ix>x then table.insert(pieces,{x,iy,ix-x,ib-iy}) end
        if ir<right then table.insert(pieces,{ir,iy,right-ir,ib-iy}) end
    end
    for index,piece in pieces do
        if piece[3]>0 and piece[4]>0 then
            local args=table.clone(request)
            args.origin_x,args.origin_y=surface_x,surface_y
            table.insert(drop_zones,ui.Box{id=id.."-"..index,position="absolute",
                left=piece[1]-surface_x,top=piece[2]-surface_y,width=piece[3],height=piece[4],
                on_drop=edit("move",args)})
        end
    end
end
-- The desktop target is input-only and split around this panel, so pointer drops
-- over its buttons and fields keep their normal behavior. No viewport is painted.
local canvas_request={host="canvas",point=true,x=0,y=0,w=sp.huge*6,h=sp.huge*2,snap=sp.huge,
    canvas_width=viewport_width,canvas_height=viewport_height}
add_outside_drop("drop-canvas",{0,context.header,viewport_width,viewport_height-context.header},canvas_request)
-- Bar targets follow the document's actual region slots; the transfer engine adds
-- source origin before the behavior maps output coordinates back to tiles.
local tile_width=viewport_width/document.hosts.top_bar.tiles
for region,bounds in document.hosts.top_bar.regions do
    add_outside_drop("drop-bar-"..region,
        {bounds.start*tile_width,0,bounds.span*tile_width,context.header},
        {host="top_bar",region=region,start=bounds.start,point=true,host_width=viewport_width,
            region_start=bounds.start,region_span=bounds.span})
end
if page=="layout" then
    table.insert(content,kit.Card{id="placement",width="fill",gap=sp.sm,text("placement-title","Placed widgets",24,tx.title),ui.Column(rows)})
elseif page=="catalogue" then
    table.insert(content,kit.Card{id="available",width="fill",gap=sp.sm,text("available-title","Widget catalogue",24,tx.title),ui.Column(catalogue)})
else
    table.insert(content,kit.Card{id="selected-settings",width="fill",gap=sp.sm,
        text("settings-title",current and ((registry.catalogue[current.widget] or {}).title or current.widget) or "Widget settings",24,tx.title),ui.Column(form),
        text("global-trigger","Default dropdown trigger: "..global_trigger,22,tx.body),
        ui.Row{id="global-trigger-options",width="fill",gap=sp.xs,
            button("default-click","Click",edit("setting",{key="dropdown_trigger",value="click"})),
            button("default-hover","Hover",edit("setting",{key="dropdown_trigger",value="hover"}))}})
end
local editor_body=ui.Column{id="editor-root",width="fill",height="fill",gap=sp.sm,
        ui.Row{id="header",width="fill",height=sp.huge+sp.lg,shrink=0,align="center",gap=sp.sm,
            text("title","Edit widgets",26,tx.title),button("undo","Undo",edit("undo")),
            button("reset","Reset",edit("reset")),button("cancel","Cancel",edit("cancel")),button("done","Done",edit("done"))},
        ui.Row{id="pages",width="fill",gap=sp.xs,shrink=0,
            button("layout","Layout",edit("page",{page="layout"})),button("catalogue","Catalogue",edit("page",{page="catalogue"})),button("settings","Settings",edit("page",{page="settings"})),
            trash},
        ui.Text{id="error",text=ui.bind("widgets.error"),height=20,font_size=tx.caption,color="error"},
        ui.Scroll{id="editor-scroll",scroll=ui.bind("widgets.scroll"),width="fill",grow=1,ui.Column(content)}}
local panel={edge="top",id="widgets-editor",width=panel_width,height=panel_height,
    metadata=require("../editing").seed(document,viewport_width,viewport_height),open=ui.bind("panel.open"),padding=sp.lg,
    table.unpack(drop_zones)}
table.insert(panel,editor_body)
if ghost then table.insert(panel,ghost) end
return kit.Panel(panel)
