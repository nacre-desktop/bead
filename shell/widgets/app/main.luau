-- SPDX-License-Identifier: GPL-3.0-only
-- The Widgets app is a catalogue first; placement and style edits stay in the page.
local context=require("../context")
local ui,kit=context.ui,context.kit
local spacing,text=kit.tokens.spacing,kit.tokens.text
local registry=require("../registry")
local preview=require("preview")
local settings_form=require("../settings-form")
local dragging=require("../dragging")

local function catalogue_descriptors()
    local packages_out={}
    for _,package in registry.catalogue do
        local descriptor={id=package.id,title=package.title,icon=package.icon,category=package.category,
            defaults=package.defaults,settings=package.settings,forms={}}
        for placement,form_def in package.forms do
            if placement~="panel" then
                local form_copy={default_style=form_def.default_style,legacy_style_setting=form_def.legacy_style_setting,styles={}}
                for name,style in form_def.styles do
                    if not style.hidden then
                        form_copy.styles[name]={title=style.title,min_span=style.min_span,max_span=style.max_span,preferred_span=style.preferred_span,preferred_height=style.preferred_height or spacing.huge*3}
                    end
                end
                descriptor.forms[placement]=form_copy
            end
        end
        table.insert(packages_out,descriptor)
    end
    table.sort(packages_out,function(a,b) return a.id<b.id end)
    return packages_out
end
local function get(key,fallback)
    local value=ui.resolve_for_check(ui.bind(key))
    return value==nil and fallback or value
end
local function document_value(value)
    if type(value)~="table" then return nil end
    if type(value[1])=="table" and type(value[1].hosts)=="table" then return value[1] end
    if type(value.hosts)=="table" then return value end
    return nil
end
local function wallpaper_preview(rows)
    for _,row in rows or {} do
        if row.kind=="wallpaper-scene" and type(row.preview)=="string" then return row.preview end
        local nested=wallpaper_preview(row.rows)
        if nested~="" then return nested end
    end
    return ""
end
local width=get("bead.viewport.width",1280)
local height=get("bead.viewport.height",800)
local screen_w=math.max(1,get("widgets.screen_width",get("bead.viewport.width",1920)))
local screen_h=math.max(1,get("widgets.screen_height",get("bead.viewport.height",1080)))
local output_w,output_h=screen_w,screen_h
local output_id=get("widgets.screen_id","")
for _,output in get("widgets.outputs",{}) do
    if output.name==output_id then
        output_w=math.max(1,tonumber(output.logical_width) or screen_w)
        output_h=math.max(1,tonumber(output.logical_height) or screen_h)
        break
    end
end
local drag_active=get("widgets.drag.active",false)==true
local drag_phase=get("widgets.drag.phase","")
local transient=document_value(get("widgets.drag.layout"))
local doc=(drag_active and drag_phase=="dragging" and transient)
    or document_value(get("widgets.draft")) or document_value(get("widgets.layout"))
    or document_value(get("widgets.initial")) or registry.default(screen_w)
local initial=document_value(get("widgets.initial")) or registry.default(screen_w)
registry.validate(doc)
local rev=get("widgets.rev",0)
local catalogue=get("widgets.catalogue",{})
if #catalogue==0 then catalogue=catalogue_descriptors() end
local selected=get("widgets.selected","")
local selected_group=get("widgets.group","")
local category=get("widgets.category","all")
local searching=get("widgets.search","")
local undo_state=get("widgets.undo",false)
local undo_available=(type(undo_state)=="table" and #undo_state>0) or undo_state==true
local message=get("widgets.message","")
local error_message=get("widgets.error","")
local busy=get("widgets.busy",false)
local wallpaper=get("widgets.wallpaper","")
if wallpaper=="" then wallpaper=wallpaper_preview(get("settings.items",{})) end
local wallpaper_path=get("widgets.wallpaper.path","")

local function navigate(op,value)
    return ui.action("widgets.navigate",{{op=op,value=value,document=doc,initial=initial,rev=rev,catalogue=catalogue}})
end
local function find_item(widget,group)
    local found
    local function scan(items)
        for _,item in items or {} do
            if item.widget==widget and (group==nil or (item.group or item.iid)==group) then found=item; return end
            scan(item.children)
            if found then return end
        end
    end
    for _,host in doc.hosts or {} do scan(host.instances) end
    return found
end
local function groups_for(widget)
    local rows,by_id={},{}
    for group,entry in doc.widgets or {} do
        if entry.widget==widget then
            by_id[group]={id=group,entry=entry,instance=find_item(widget,group)}
        end
    end
    local function scan(items)
        for _,item in items or {} do
            if item.widget==widget then
                local group=item.group or item.iid
                by_id[group]=by_id[group] or {id=group,entry={widget=widget,settings=item.settings or {},forms={}},instance=item}
                if not by_id[group].instance then by_id[group].instance=item end
            end
            scan(item.children)
        end
    end
    for _,host in doc.hosts or {} do scan(host.instances) end
    local ids={}
    for group in by_id do table.insert(ids,group) end
    table.sort(ids)
    local labels={}
    for _,group in ids do
        local row=by_id[group]
        local item=row.instance
        local title="Desktop"
        if item and item.host~="canvas" then
            local region=item.slot and item.slot.region or "left"
            local region_title=({left="Left",centre="Center",right="Right"})[region] or "Left"
            title="Bar · "..region_title
        end
        labels[title]=(labels[title] or 0)+1
        row.title=title
        table.insert(rows,row)
    end
    local totals=table.clone(labels)
    for _,row in rows do
        labels[row.title]=labels[row.title]-1
        if totals[row.title]>1 then row.label=row.title.." · "..tostring(totals[row.title]-labels[row.title])
        else row.label=row.title end
    end
    return rows
end
local function active_group(widget,requested)
    local rows=groups_for(widget)
    for _,row in rows do if row.id==requested then return row.id end end
    return rows[1] and rows[1].id or widget
end
local function widget_group(widget)
    return active_group(widget,widget==selected and selected_group or nil)
end
local function edit(request)
    if type(request.widget)=="string" and request.group==nil then request.group=widget_group(request.widget) end
    request.document=doc
    request.initial=initial
    request.rev=rev
    request.catalogue=catalogue
    request.screen_width=screen_w
    request.screen_height=screen_h
    return ui.action("widgets.edit",{request})
end
local function label(id,value,size,color,props)
    props=props or {}
    props.id=id
    props.text=value or ""
    props.font_size=size or text.body*4/3
    props.width=props.width or (props.grow and 0 or math.ceil(ui.text_width(props.text,"IBM Plex Sans",props.font_size))+spacing.hairline)
    props.color=color or "onSurface"
    return ui.Text(props)
end
local function button(id,value,action,props)
    props=props or {}
    props.id=id
    props.text=value
    props.height=props.height or spacing.huge+spacing.xs
    props.width=props.width or math.ceil(ui.text_width(value,props.family or "IBM Plex Sans",props.font_size or text.body*4/3))+2*(props.padding or spacing.sm)
    props.padding=props.padding or spacing.sm
    props.radius=props.radius or kit.tokens.radii.small
    props.fill=props.fill or "surfaceContainerHigh"
    props.hover_fill=props.hover_fill or "surfaceContainerHighest"
    props.selected_fill=props.selected_fill or ui.alpha("primary",.18)
    props.focus_on_pointer=false
    local enabled=props.enabled~=false
    props.enabled=nil
    props.on_activate=enabled and action or nil
    if not enabled then props.color="onSurfaceVariant"; props.fill=ui.alpha(props.fill,.45) end
    return kit.Button(props)
end

local categories={
    {id="all",label=ui.tr("bead.widgets.category-all")},
    {id="bar",label=ui.tr("bead.widgets.category-bar")},
    {id="status",label=ui.tr("bead.widgets.category-status")},
    {id="media",label=ui.tr("bead.widgets.category-media")},
    {id="system",label=ui.tr("bead.widgets.category-system")},
    {id="containers",label=ui.tr("bead.widgets.category-containers")},
    {id="yours",label=ui.tr("bead.widgets.category-yours")},
}
local widget_nav_ids={"clock","battery","sound","power","notifications","bluetooth","wifi","launcher","updates","workspaces","title","pill"}
for _,id in registry.plugins.widgets do
    if not table.find(widget_nav_ids,id) then table.insert(widget_nav_ids,id) end
end
local function in_document(widget)
    for group,entry in doc.widgets or {} do
        if entry.widget==widget then return true end
    end
    local found=false
    local function scan(items)
        for _,item in items or {} do
            if item.widget==widget then found=true; return end
            scan(item.children)
            if found then return end
        end
    end
    for _,host in doc.hosts or {} do scan(host.instances) end
    return found
end
local function matches_category(item)
    if category=="all" then return true end
    if category=="yours" then return in_document(item.id) end
    if category=="bar" then return item.id=="launcher" or item.id=="workspaces" or item.id=="title" end
    if category=="status" then return table.find({"updates","sound","wifi","bluetooth","battery","notifications"},item.id)~=nil end
    if category=="media" then return item.id=="sound" end
    if category=="system" then return item.id=="power" or item.id=="updates" end
    if category=="containers" then return item.id=="pill" end
    return false
end
local function descriptors()
    local rows={}
    for id,package in registry.catalogue do
        table.insert(rows,{id=id,title=package.title,description=package.description,icon=package.icon,category=package.category})
    end
    table.sort(rows,function(a,b) return a.title<b.title end)
    return rows
end
local packages=descriptors()
local function visible_packages()
    local query=string.lower(searching or "")
    local rows={}
    for _,item in packages do
        local hit=query=="" or string.find(string.lower(item.title),query,1,true)
            or string.find(string.lower(item.description or ""),query,1,true)
            or string.find(string.lower(item.id),query,1,true)
        if hit and matches_category(item) then table.insert(rows,item) end
    end
    return rows
end

local function style_name(package,placement)
    local forms=package.forms and package.forms[placement]
    return forms and forms.default_style or ""
end
local RAIL=math.min(spacing.huge*7+spacing.xl,math.max(spacing.huge*4,width*.28))
local gallery_grid_width=math.max(1,width-RAIL-3*spacing.lg)
local CARD_W=math.max(160,(gallery_grid_width-spacing.md)/2)
local PREVIEW_W=math.max(48,CARD_W-2*spacing.md-50-spacing.sm)
local function gallery_card(item)
    local package=registry.catalogue[item.id]
    local bar=preview.render(item.id,"bar",style_name(package,"bar"),package.defaults,PREVIEW_W,36)
    local desktop=package.forms.desktop and preview.render(item.id,"desktop",style_name(package,"desktop"),package.defaults,PREVIEW_W,48)
    -- B0198b: the Overview's bar and desktop previews are drag sources too (grip at the right).
    local function source(form,node)
        local payload=dragging.app_payload(doc,item.id,form,style_name(package,form),package.defaults,nil)
        return ui.Box{id=form.."-preview-frame",width="fill",height=form=="bar" and 40 or 48,radius=kit.tokens.radii.small,
            fill="surfaceContainerHighest",clip_children=true,align="center",justify="center",
            drag_payload=payload and {payload} or nil,on_drag=payload and ui.action("widgets.drag") or nil,
            node,ui.Text{id="grip",position="absolute",right=spacing.xs,top=spacing.xs,text="drag_indicator",
                family="Material Symbols Rounded",font_size=kit.tokens.icons.medium,width=kit.tokens.icons.medium+2,
                height=kit.tokens.icons.medium+2,color="onSurfaceVariant"}}
    end
    return ui.Button{id="widget-card-"..item.id,width="fill",height=192,shrink=0,padding=spacing.sm,
        radius=kit.tokens.radii.medium,fill="surfaceContainer",hover_fill="surfaceContainerHigh",activate_on_release=true,
        selected=selected==item.id,selected_fill=ui.alpha("primary",.12),focus_on_pointer=false,
        aria_label="Open "..item.title,on_activate=navigate("select",item.id),
        ui.Column{id="card-content",width="fill",height="fill",gap=spacing.xs,
            ui.Row{id="card-heading",width="fill",height=38,align="center",gap=spacing.sm,
                ui.Text{id="card-icon",text=item.icon or "widgets",family="Material Symbols Rounded",font_size=kit.tokens.icons.medium,width=24,height=24,color="primary"},
                ui.Column{id="card-copy",width=0,grow=1,gap=spacing.hairline,justify="center",
                    label("card-title",item.title,text.title*4/3,"onSurface",{width="fill",height=20}),
                    label("card-description",package.description,text.caption*4/3,"onSurfaceVariant",{width="fill",height=16})},
                ui.Text{id="card-open",text="chevron_right",family="Material Symbols Rounded",font_size=kit.tokens.icons.medium,width=20,height=24,color="onSurfaceVariant"}},
            ui.Row{id="bar-preview-row",width="fill",height=48,gap=spacing.sm,align="center",
                label("bar-label","Bar",text.caption*4/3,"onSurfaceVariant",{width=50,height=18}),
                source("bar",bar)},
            ui.Row{id="desktop-preview-row",width="fill",height=56,gap=spacing.sm,align="center",
                label("desktop-label",desktop and "Desktop" or "Bar only",text.caption*4/3,"onSurfaceVariant",{width=50,height=18}),
                desktop and source("desktop",desktop)
                    or ui.Box{id="desktop-empty",width="fill",height=48,radius=kit.tokens.radii.small,fill="surfaceContainerHigh",
                        label("desktop-empty-copy","This widget stays in the bar",text.caption*4/3,"onSurfaceVariant",{width="fill",height=18,text_align="center"})}},
        }}
end

local function category_button(item)
    local active=category==item.id
    local button_width=math.ceil(ui.text_width(item.label,"IBM Plex Sans",text.caption*4/3))+2*spacing.md
    return ui.Button{id="category-"..item.id,width=button_width,height=spacing.huge+spacing.sm,padding_left=spacing.md,padding_right=spacing.md,
        radius=kit.tokens.radii.small,fill=active and ui.alpha("primary",.15) or "transparent",
        hover_fill="surfaceContainerHigh",selected=active,focus_on_pointer=false,
        aria_label=item.label,on_activate=navigate("category",item.id),
        label("category-label",item.label,text.caption*4/3,active and "onSurface" or "onSurfaceVariant",{height=18,text_align="center"})}
end

local function gallery()
    local rows=visible_packages()
    local category_nodes={id="category-list",width="fill",gap=spacing.xs,align="center"}
    for _,item in categories do table.insert(category_nodes,category_button(item)) end
    local card_nodes={id="widget-grid",width="fill",columns=2,gap=spacing.md,row_height=220}
    for _,item in rows do table.insert(card_nodes,gallery_card(item)) end
    return ui.Column{id="gallery-page",width="fill",height="fill",gap=spacing.md,
        ui.Column{id="gallery-content",width="fill",height="fill",gap=spacing.md,
            ui.Row{id="gallery-title-row",width="fill",height=54,shrink=0,align="center",
                ui.Column{id="gallery-title-copy",width=0,grow=1,gap=spacing.xs,
                    label("gallery-title",ui.tr("bead.widgets.choose"),text.headline*4/3,"onSurface",{width="fill",height=28}),
                    label("gallery-subtitle",ui.tr("bead.widgets.choose-hint"),text.caption*4/3,"onSurfaceVariant",{width="fill",height=18})},
                label("widget-count",tostring(#rows).." widgets",text.caption*4/3,"onSurfaceVariant",{height=18})},
            ui.Row(category_nodes),
            #rows==0 and ui.Box{id="gallery-empty",grow=1,width="fill",radius=kit.tokens.radii.medium,fill="surfaceContainer",
                label("gallery-empty-text","No widgets match this search.",text.body*4/3,"onSurfaceVariant",{width="fill",height=24,text_align="center"})}
                or ui.Scroll{id="gallery-scroll",width="fill",grow=1,scroll=ui.bind("widgets.scroll"),
                    ui.Grid(card_nodes)}}
    }
end

local function active_settings(package,entry,instance)
    local values=table.clone(package.defaults)
    for key,value in entry.settings or instance and instance.settings or {} do values[key]=value end
    return values
end
local function form_action(widget,placement,group)
    return function(op,fields)
        local request={op=op,widget=widget,group=group,form=placement}
        for key,value in fields or {} do
            if key~="iid" then request[key]=value end
        end
        return edit(request)
    end
end
local STYLE_PREVIEW_HEIGHT=spacing.huge*2
local STYLE_TILE_HEIGHT=STYLE_PREVIEW_HEIGHT+text.caption+spacing.md
local STYLE_SECTION_HEIGHT=2*spacing.md+22+spacing.sm+STYLE_TILE_HEIGHT
local function new_style(widget,placement,package)
    local chosen=get("widgets.new_style",{})
    local name=type(chosen)=="table" and chosen[widget.."/"..placement] or nil
    local styles=package.forms[placement].styles
    if type(name)=="string" and styles[name] and not styles[name].hidden then return name end
    return package.forms[placement].default_style
end
-- B0208: a gallery card is a drag source for a NEW copy; clicking it only chooses the style
-- that "Add" uses. A placed copy's own style cards (on_activate given) restyle that copy only.
local function style_tile(widget,placement,package,style_id,style,settings,is_selected,on_activate,draggable,columns)
    local panel_width=(width-RAIL-3*spacing.lg)/(draggable and 2 or 1)-2*spacing.md
    local preview_width=math.max(spacing.xxl*2,math.min(spacing.huge*3+spacing.xl,
        (panel_width-(columns-1)*spacing.sm)/columns-2*spacing.xs))
    local tile_width=preview_width+2*spacing.sm
    local tile_preview=preview.render(widget,placement,style_id,settings,preview_width,STYLE_PREVIEW_HEIGHT)
    local payload=draggable and dragging.app_payload(doc,widget,placement,style_id,settings,nil) or nil
    local props={id="style-"..style_id..(placement=="desktop" and "-desktop" or "")..(draggable and "" or "-placed"),
        width=tile_width,height=STYLE_TILE_HEIGHT,shrink=0,
        radius=kit.tokens.radii.small,fill=is_selected and ui.alpha("primary",.15) or "surfaceContainer",
        border_width=is_selected and 1.5 or 0,border_color=is_selected and "primary" or nil,
        hover_fill="surfaceContainerHigh",selected=is_selected,focus_on_pointer=false,
        aria_label=style.title,on_activate=on_activate,
        drag_payload=payload and {payload} or nil,on_drag=payload and ui.action("widgets.drag") or nil}
    if payload then
        table.insert(props,ui.Text{id="style-grip",position="absolute",right=spacing.xs,top=spacing.xs,text="drag_indicator",
            family="Material Symbols Rounded",font_size=kit.tokens.icons.medium,width=kit.tokens.icons.medium+2,
            height=kit.tokens.icons.medium+2,color="onSurfaceVariant"})
    end
    table.insert(props,ui.Column{id="style-card",width="fill",height="fill",gap=spacing.xs,align="center",
        ui.Box{id="style-preview",width=preview_width,height=STYLE_PREVIEW_HEIGHT,align="center",justify="center",
            radius=kit.tokens.radii.small,fill="surfaceContainerHighest",clip_children=true,tile_preview},
        label("style-title",style.title,text.caption*4/3,"onSurface",{width=tile_width,height=14,text_align="center"})})
    return ui.Button(props)
end
local function style_tiles(widget,placement,package,style_id,settings,activate,draggable)
    local forms=package.forms[placement]
    local keys={}
    for key,style in forms.styles do if not style.hidden then table.insert(keys,key) end end
    table.sort(keys)
    local columns=math.max(1,math.min(draggable and 4 or 6,#keys))
    local tiles={id=draggable and "style-tiles" or "placed-style-tiles",width="fill",height=STYLE_TILE_HEIGHT,columns=columns,
        gap=spacing.sm,row_height=STYLE_TILE_HEIGHT,shrink=0}
    for _,key in keys do
        table.insert(tiles,style_tile(widget,placement,package,key,forms.styles[key],settings,key==style_id,
            activate(key),draggable,columns))
    end
    return ui.Grid(tiles)
end
-- B0198: one live screen preview at the real output aspect: the real bar with its
-- widgets and every desktop widget at its slot. Large enough that the bar reads.
local function stage_geometry()
    local stage_w=math.max(1,width-RAIL-3*spacing.lg)
    local stage_h=math.max(spacing.huge*4,math.min(stage_w*output_h/output_w,height*.62))
    local scale=math.min(stage_w/output_w,stage_h/output_h)
    local mini_w,mini_h=output_w*scale,output_h*scale
    return {w=stage_w,h=stage_h,scale=scale,mini_w=mini_w,mini_h=mini_h,
        left=(stage_w-mini_w)/2,top=(stage_h-mini_h)/2,bar_h=math.max(1,get("bar.height",50))}
end
local function stage_backdrop(stage)
    local children={ui.Box{id="stage-background",position="absolute",left=stage.left,top=stage.top,
        width=stage.mini_w,height=stage.mini_h,fill="surfaceContainerHighest"}}
    local source=wallpaper~="" and wallpaper or wallpaper_path
    if source~="" then
        table.insert(children,ui.Image{id="stage-wallpaper",position="absolute",left=stage.left,top=stage.top,
            width=stage.mini_w,height=stage.mini_h,icon=source,image_fit="cover"})
    end
    table.insert(children,ui.Box{id="stage-tint",position="absolute",left=stage.left,top=stage.top,
        width=stage.mini_w,height=stage.mini_h,fill=ui.alpha("surface",.08)})
    return children
end
local function stage_desktop(stage,selected_iid)
    local nodes={}
    local canvas=doc.hosts.canvas
    for _,item in canvas and canvas.instances or {} do
        local slot=item.slot
        local node=preview.render_instance(item,"desktop",stage.scale)
        if node and slot then
            local own=item.iid==selected_iid
            table.insert(nodes,ui.Box{id="stage-desktop-"..tostring(item.iid),position="absolute",
                left=stage.left+slot.x*stage.scale,top=stage.top+slot.y*stage.scale,
                width=slot.w*stage.scale,height=slot.h*stage.scale,clip_children=true,
                radius=kit.tokens.radii.small,border_width=own and 2 or 0,border_color="primary",node})
        end
    end
    return nodes
end
local function widget_sidebar()
    local nav={id="widget-list",width="fill",gap=spacing.xs}
    local nav_query=string.lower(searching or "")
    local function row(id,title,description,icon,active,action,accent)
        return ui.Button{id="widget-nav-"..id,width="fill",height=spacing.huge+spacing.xxl,shrink=0,
            padding_left=spacing.sm,padding_right=spacing.xs,radius=kit.tokens.radii.small,
            fill=active and ui.alpha("primary",.15) or "transparent",hover_fill="surfaceContainerHigh",
            selected=active,focus_on_pointer=false,aria_label=title,on_activate=action,
            ui.Row{id="widget-nav-content",width="fill",height="fill",align="center",gap=spacing.sm,
                ui.Text{id="widget-nav-icon",text=icon or "widgets",family="Material Symbols Rounded",
                    font_size=kit.tokens.icons.large*4/3,width=24,height=28,color=(active or accent) and "primary" or "onSurfaceVariant"},
                ui.Column{id="widget-nav-copy",width=0,grow=1,height="fill",justify="center",gap=spacing.hairline,
                    label("widget-nav-title",title,text.body*4/3,active and "onSurface" or "onSurfaceVariant",{width="fill",height=18}),
                    type(description)=="string" and description~="" and label("widget-nav-description",description,text.caption*4/3,"onSurfaceVariant",{width="fill",height=30}) or nil}}}
    end
    -- The Overview is a section of its own: accent icon, then a rule and a group label.
    table.insert(nav,row("overview",ui.tr("bead.widgets.overview"),"","dashboard",selected=="",navigate("back",selected),true))
    table.insert(nav,ui.Box{id="widget-nav-rule",width="fill",height=1,shrink=0,fill="outlineVariant"})
    table.insert(nav,label("widget-nav-group",ui.tr("bead.widgets.group-label"),text.caption*4/3,"onSurfaceVariant",
        {width="fill",height=18,shrink=0,padding_left=spacing.sm}))
    -- Spec 0203: the Plugins page is its own route under the widget list.
    local plugins_open=string.sub(selected,1,8)=="@plugins"
    for _,id in widget_nav_ids do
        local package=registry.catalogue[id]
        if package and (nav_query=="" or string.find(string.lower(package.title),nav_query,1,true)
            or string.find(string.lower(package.description),nav_query,1,true)
            or string.find(string.lower(id),nav_query,1,true)) then
            table.insert(nav,row(id,package.title,package.description,package.icon,selected==id,navigate("select",id)))
        end
    end
    table.insert(nav,ui.Box{id="widget-nav-plugins-rule",width="fill",height=1,shrink=0,fill="outlineVariant"})
    table.insert(nav,row("plugins","Plugins","Add, enable and configure plugins","extension",plugins_open,navigate("select","@plugins")))
    return ui.Column{id="widget-sidebar",width=RAIL,height="fill",shrink=0,gap=spacing.sm,padding=spacing.md,
        radius=kit.tokens.radii.medium,fill="surfaceContainer",
        label("widget-sidebar-title",ui.tr("bead.widgets.side-title"),text.title*4/3,"onSurface",{height=26}),
        kit.TextInput{id="widget-sidebar-search",width="fill",height=spacing.huge+spacing.md,text=ui.bind("widgets.search"),
            placeholder=ui.tr("bead.widgets.find"),fill="surface",border_width=1,border_color="outlineVariant",
            aria_label=ui.tr("bead.widgets.find")},
        ui.Scroll{id="widget-sidebar-scroll",width="fill",grow=1,scroll=ui.bind("widgets.nav.scroll"),ui.Column(nav)}}
end

local function section(id,props,...)
    props.id=id; props.width=props.width or "fill"; props.shrink=0
    props.padding=props.padding or spacing.md; props.radius=kit.tokens.radii.medium; props.fill=props.fill or "surfaceContainer"
    props.gap=props.gap or spacing.sm
    for index=1,select("#",...) do
        local child=select(index,...)
        if child then table.insert(props,child) end
    end
    return ui.Column(props)
end
local function where(item)
    if not item then return "" end
    if item.host=="canvas" then return ui.tr("bead.widgets.where-desktop") end
    local region=item.slot and item.slot.region or "left"
    local side=({left="bead.widgets.where-left",centre="bead.widgets.where-centre",right="bead.widgets.where-right"})[region]
        or "bead.widgets.where-left"
    return ui.tr(side)
end
-- B0208: the page has two jobs, kept apart. "Add" holds the gallery (drag a style card, or
-- choose one and press Add). "On your screen" edits one placed copy at a time: the copy the
-- user clicked on the bar or desktop in edit mode, or picked here.
local function detail()
    local package=registry.catalogue[selected]
    if not package then return gallery() end
    local placed={}
    for _,row in groups_for(selected) do if row.instance then table.insert(placed,row) end end
    local current
    for _,row in placed do if row.id==selected_group then current=row end end
    current=current or placed[1]
    local instance=current and current.instance
    local placement=instance and (instance.host=="canvas" and "desktop" or "bar") or "bar"
    local group=current and current.id or selected
    local settings=active_settings(package,current and current.entry or {},instance)
    local desktop_available=package.forms.desktop~=nil
    local new_bar=new_style(selected,"bar",package)
    local new_desktop=desktop_available and new_style(selected,"desktop",package) or ""
    local defaults=table.clone(package.defaults)
    local function choose(form_name)
        return function(style_id)
            return ui.action("widgets.navigate",{{op="new-style",widget=selected,form=form_name,value=style_id}})
        end
    end
    local function add_column(form_name,chosen)
        return section(form_name.."-style-section",{width=0,grow=1,height=STYLE_SECTION_HEIGHT},
            ui.Row{id=form_name.."-style-heading-row",width="fill",height=22,shrink=0,align="center",gap=spacing.sm,
                label(form_name.."-style-heading",form_name=="bar" and "Bar" or "Desktop",text.body*4/3,"onSurface",{height=22}),
                ui.Text{id=form_name.."-drag-icon",text="drag_indicator",family="Material Symbols Rounded",font_size=kit.tokens.icons.medium,
                    width=kit.tokens.icons.medium+2,height=20,color="primary"},
                label(form_name.."-drag-hint",ui.tr(form_name=="bar" and "bead.widgets.drag-to-bar" or "bead.widgets.drag-to-desktop"),
                    text.caption*4/3,"onSurfaceVariant",{width=0,grow=1,height=18}),
                button(form_name.."-add",ui.tr(form_name=="bar" and "bead.widgets.add-bar" or "bead.widgets.add-desktop"),
                    edit({op="add",widget=selected,form=form_name,style=chosen}),{height=22,padding=spacing.xs,font_size=text.caption*4/3})},
            style_tiles(selected,form_name,package,chosen,defaults,choose(form_name),true))
    end
    local content_props={id="detail-content",width="fill",shrink=0,gap=spacing.md,
        label("drag-hint",ui.tr("bead.widgets.drag-hint"),text.caption*4/3,"onSurfaceVariant",{width="fill",height=18,shrink=0}),
        ui.Row{id="form-style-columns",width="fill",height=STYLE_SECTION_HEIGHT,shrink=0,gap=spacing.md,
            add_column("bar",new_bar),desktop_available and add_column("desktop",new_desktop) or nil}}
    -- On your screen: the placed copies, then the selected copy's own style, colour and settings.
    local copies={id="instance-options",width="fill",height=spacing.huge+spacing.xs,gap=spacing.sm}
    local seen={}
    for _,row in placed do
        local name=where(row.instance)
        seen[name]=(seen[name] or 0)+1
        local caption=seen[name]>1 and name.." · "..tostring(seen[name]) or name
        table.insert(copies,button("instance-"..row.id,caption,navigate("instance",row.id),{
            selected=row==current,fill=row==current and ui.alpha("primary",.15) or "surfaceContainerHigh"}))
    end
    local editor
    if instance then
        local act=form_action(selected,placement,group)
        local settings_package=table.clone(package)
        settings_package.settings={}
        for key,schema in package.settings do
            if key~="dropdown_trigger" and key~="colour_role" and not schema.style_alias and not schema.hidden then
                settings_package.settings[key]=schema
            end
        end
        local item={iid=group,widget=selected,settings=settings}
        local fields=settings_form.rows(item,settings_package,act)
        local current_style=instance.style
        local styles=package.forms[placement].styles
        if type(current_style)~="string" or not styles[current_style] then current_style=package.forms[placement].default_style end
        editor=section("instance-editor",{fill="surfaceContainerHigh"},
            ui.Row{id="instance-editor-heading",width="fill",height=spacing.huge+spacing.xs,shrink=0,align="center",gap=spacing.sm,
                label("instance-editor-title",package.title.." · "..where(instance),text.title*4/3,"onSurface",{width=0,grow=1,height=24}),
                button("instance-remove",ui.tr("bead.widgets.remove-copy"),
                    edit({op="remove",widget=selected,group=group,iid=instance.iid,form=placement}),{fill="surfaceContainerHighest"})},
            style_tiles(selected,placement,package,current_style,settings,function(style_id)
                return edit({op="style",widget=selected,group=group,iid=instance.iid,form=placement,style=style_id})
            end,false),
            settings_form.row(item,"colour_role",package.settings.colour_role,settings.colour_role,act),
            #fields.children>0 and fields or nil)
    end
    table.insert(content_props,section("placed-section",{},
        label("placed-title",ui.tr("bead.widgets.on-screen"),text.title*4/3,"onSurface",{height=24}),
        #placed>0 and label("placed-hint",ui.tr("bead.widgets.on-screen-hint"),text.caption*4/3,"onSurfaceVariant",{width="fill",height=18})
            or label("placed-empty",ui.tr("bead.widgets.on-screen-empty"),text.caption*4/3,"onSurfaceVariant",{width="fill",height=18}),
        #placed>0 and ui.Row(copies) or nil,
        editor))
    -- The live screen preview outlines the copy being edited.
    local stage=stage_geometry()
    local bar_node=preview.render_bar(doc,output_w,stage.bar_h,stage.scale,
        instance and instance.host~="canvas" and selected or nil,instance and group or nil)
    local stage_children=stage_backdrop(stage)
    table.insert(stage_children,ui.Box{id="stage-bar",position="absolute",left=stage.left,top=stage.top,
        width=stage.mini_w,height=stage.bar_h*stage.scale,clip_children=true,fill="surface",bar_node})
    for _,node in stage_desktop(stage,instance and instance.iid) do table.insert(stage_children,node) end
    table.insert(stage_children,ui.Column{id="stage-caption",position="absolute",left=spacing.md,bottom=spacing.md,gap=spacing.xs,
        label("stage-title","Your screen",text.caption*4/3,"onSurface",{height=18}),
        label("stage-size",tostring(output_w).." × "..tostring(output_h),text.caption*4/3,"onSurfaceVariant",{height=16})})
    local stage_props={id="preview-stage",width="fill",height=stage.h,shrink=0,clip_children=true,
        radius=kit.tokens.radii.medium,fill="surfaceContainerHigh"}
    for _,child in stage_children do table.insert(stage_props,child) end
    table.insert(content_props,ui.Box(stage_props))
    table.insert(content_props,section("dropdown-trigger",{gap=spacing.xs},
        label("trigger-label","Open dropdowns on",text.body*4/3,"onSurface",{height=22}),
        ui.Row{id="trigger-choices",width="fill",height=spacing.huge+spacing.xs,gap=spacing.xs,
            button("trigger-click","Click",edit({op="default-trigger",value="click"}),{
                width=0,grow=1,selected=(doc.dropdown_trigger or "click")=="click"}),
            button("trigger-hover","Hover",edit({op="default-trigger",value="hover"}),{
                width=0,grow=1,selected=doc.dropdown_trigger=="hover"})}))
    if error_message~="" or message~="" then
        table.insert(content_props,label("save-status",error_message~="" and error_message or message,
            text.caption*4/3,error_message~="" and "error" or "onSurfaceVariant",{width="fill",height=20}))
    end
    return ui.Column{id="widget-detail",width=0,grow=1,height="fill",gap=spacing.md,
        ui.Row{id="detail-heading",width="fill",height=54,shrink=0,gap=spacing.md,align="center",
            ui.Text{id="detail-icon",text=package.icon or "widgets",family="Material Symbols Rounded",font_size=kit.tokens.icons.large,width=36,height=36,color="primary"},
            ui.Column{id="detail-title-copy",width=0,grow=1,gap=spacing.xs,
                label("detail-title",package.title,text.headline*4/3,"onSurface",{width="fill",height=28}),
                label("detail-description",package.description,text.caption*4/3,"onSurfaceVariant",{height=18})}},
        ui.Scroll{id="detail-scroll",width="fill",grow=1,scroll=ui.bind("widgets.detail.scroll"),ui.Column(content_props)}
    }
end

local function metadata()
    return {{widget_layout=true,widget_clock_preview_seconds=selected=="clock",document=doc,initial=initial,rev=rev,catalogue=catalogue,
        screen_width=screen_w,screen_height=screen_h,grid_cell=dragging.grid_cell(),
        viewport_width=width,viewport_height=height}}
end

-- B0210: an ordinary window: niri draws (and clips to) the rounded border and focus ring.
local app_content=ui.Column{id="widgets-app",width="100%",height="100%",fill="surface",clip_children=true,
    ui.Column{id="app-header-box",width="fill",height=65,shrink=0,
        ui.Row{id="app-header",width="fill",height=64,shrink=0,align="center",gap=spacing.lg,padding_left=spacing.xl,padding_right=spacing.xl,
            ui.Image{id="app-mark",path="widgets/app/assets/nacre-widgets.themed.svg",width=44,height=44,shrink=0,aria_label=ui.tr("bead.widgets.app-title")},
            label("app-title",ui.tr("bead.widgets.app-title"),text.title*4/3,"onSurface",{grow=1,height=30}),
            button("undo","Undo",edit({op="undo"}),{enabled=undo_available and not busy}),
            button("reset","Reset to Nacre default",edit({op="reset"}),{fill="surfaceContainerHigh",enabled=not busy})},
        ui.Box{id="header-rule",width="fill",height=1,fill="outlineVariant"}},
    ui.Row{id="app-body",width="fill",height="fill",gap=spacing.lg,padding=spacing.lg,
        widget_sidebar(),
        selected=="" and gallery() or string.sub(selected,1,8)=="@plugins" and require("plugins").page(selected,navigate) or detail()},
}
return ui.Box{id="widgets-app-root",width="100%",height="100%",metadata=metadata(),app_content}
