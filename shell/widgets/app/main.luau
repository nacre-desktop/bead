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
        output_w=math.max(1,tonumber(output.width) or screen_w)
        output_h=math.max(1,tonumber(output.height) or screen_h)
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
local form=get("widgets.form","bar")
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
    return ui.Button{id="widget-card-"..item.id,width="fill",height=192,shrink=0,padding=spacing.sm,
        radius=kit.tokens.radii.medium,fill="surfaceContainer",hover_fill="surfaceContainerHigh",
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
                ui.Box{id="bar-preview-frame",width="fill",height=40,radius=kit.tokens.radii.small,fill="surfaceContainerHighest",clip_children=true,bar}},
            ui.Row{id="desktop-preview-row",width="fill",height=56,gap=spacing.sm,align="center",
                label("desktop-label",desktop and "Desktop" or "Bar only",text.caption*4/3,"onSurfaceVariant",{width=50,height=18}),
                desktop and ui.Box{id="desktop-preview-frame",width="fill",height=48,radius=kit.tokens.radii.small,fill="surfaceContainerHighest",clip_children=true,desktop}
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

local function config_for(widget,requested_group)
    local group=active_group(widget,requested_group)
    local rows=groups_for(widget)
    for _,row in rows do
        if row.id==group then
            return group,row.entry,row.instance
        end
    end
    return group,{widget=widget,settings={},forms={}},nil
end
local function form_state(widget,placement,package,requested_group)
    local group,entry,instance=config_for(widget,requested_group)
    local config=entry.forms and entry.forms[placement] or {}
    local active=instance~=nil and ((instance.host=="canvas" and "desktop" or "bar")==placement)
    if config.enabled~=nil then active=config.enabled end
    local host=placement=="bar" and doc.hosts.top_bar or doc.hosts.canvas
    local configured=config.style
    if configured==nil or configured=="" then
        configured=nil
        local function matching(items)
            for _,item in items or {} do
                if item.widget==widget and (item.group or item.iid)==group then configured=item.style or (package.forms[placement].legacy_style_setting and (item.settings or {})[package.forms[placement].legacy_style_setting]); return true end
                if matching(item.children) then return true end
            end
            return false
        end
        matching(host and host.instances)
    end
    local selected=configured or style_name(package,placement)
    local selected_descriptor=package.forms[placement].styles[selected]
    if selected_descriptor and selected_descriptor.hidden then selected=package.forms[placement].default_style end
    return group,entry,instance,config,active,selected
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
local function style_tile(widget,placement,package,style_id,style,settings,is_selected)
    local count=0
    for _ in package.forms[placement].styles do count+=1 end
    local columns=math.max(1,math.min(4,count))
    local panel_width=(width-RAIL-3*spacing.lg)/2-2*spacing.md
    local preview_width=math.max(spacing.xxl*2,math.min(spacing.huge*4,
        (panel_width-(columns-1)*spacing.sm)/columns-2*spacing.xs))
    local tile_width=preview_width+2*spacing.sm
    local preview_height=STYLE_PREVIEW_HEIGHT
    local tile_height=STYLE_TILE_HEIGHT
    local tile_preview=preview.render(widget,placement,style_id,settings,preview_width,preview_height)
    local tile_id="style-"..style_id..(placement=="desktop" and "-desktop" or "")
    return ui.Button{id=tile_id,width=tile_width,height=tile_height,shrink=0,
        radius=kit.tokens.radii.small,fill=is_selected and ui.alpha("primary",.15) or "surfaceContainer",
        border_width=is_selected and 1.5 or 0,border_color=is_selected and "primary" or nil,
        hover_fill="surfaceContainerHigh",selected=is_selected,focus_on_pointer=false,
        aria_label=style.title,on_activate=edit({op="style",widget=widget,form=placement,style=style_id}),
        ui.Column{id="style-card",width="fill",height="fill",gap=spacing.xs,align="center",
            ui.Box{id="style-preview",width=preview_width,height=preview_height,align="center",justify="center",
                radius=kit.tokens.radii.small,fill="surfaceContainerHighest",clip_children=true,tile_preview},
            label("style-title",style.title,text.caption*4/3,"onSurface",{width=preview_width,height=14,text_align="center"})}}
end
local function style_tiles(widget,placement,package,style_id,settings)
    local forms=package.forms[placement]
    local keys={}
    for key,style in forms.styles do if not style.hidden then table.insert(keys,key) end end
    table.sort(keys)
    local tiles={id="style-tiles",width="fill",height=STYLE_TILE_HEIGHT,columns=math.max(1,math.min(4,#keys)),
        gap=spacing.sm,row_height=STYLE_TILE_HEIGHT,shrink=0}
    for _,key in keys do table.insert(tiles,style_tile(widget,placement,package,key,forms.styles[key],settings,key==style_id)) end
    return ui.Grid(tiles)
end

local function widget_sidebar()
    local nav={id="widget-list",width="fill",gap=spacing.xs}
    local nav_query=string.lower(searching or "")
    local function row(id,title,description,icon,active,action)
        return ui.Button{id="widget-nav-"..id,width="fill",height=spacing.huge+spacing.xl,shrink=0,
            padding_left=spacing.sm,padding_right=spacing.xs,radius=kit.tokens.radii.small,
            fill=active and ui.alpha("primary",.15) or "transparent",hover_fill="surfaceContainerHigh",
            selected=active,focus_on_pointer=false,aria_label=title,on_activate=action,
            ui.Row{id="widget-nav-content",width="fill",height="fill",align="center",gap=spacing.sm,
                ui.Text{id="widget-nav-icon",text=icon or "widgets",family="Material Symbols Rounded",
                    font_size=kit.tokens.icons.small,width=22,height=24,color=active and "primary" or "onSurfaceVariant"},
                ui.Column{id="widget-nav-copy",width=0,grow=1,height="fill",justify="center",gap=spacing.hairline,
                    label("widget-nav-title",title,text.body*4/3,active and "onSurface" or "onSurfaceVariant",{width="fill",height=18}),
                    label("widget-nav-description",description or "",text.caption*4/3,"onSurfaceVariant",{width="fill",height=16})}}}
    end
    table.insert(nav,row("overview",ui.tr("bead.widgets.overview"),"","widgets",selected=="",navigate("back",selected)))
    for _,id in widget_nav_ids do
        local package=registry.catalogue[id]
        if package and (nav_query=="" or string.find(string.lower(package.title),nav_query,1,true)
            or string.find(string.lower(package.description),nav_query,1,true)
            or string.find(string.lower(id),nav_query,1,true)) then
            table.insert(nav,row(id,package.title,package.description,package.icon,selected==id,navigate("select",id)))
        end
    end
    return ui.Column{id="widget-sidebar",width=RAIL,height="fill",shrink=0,gap=spacing.sm,padding=spacing.md,
        radius=kit.tokens.radii.medium,fill="surfaceContainer",
        label("widget-sidebar-title",ui.tr("bead.widgets.side-title"),text.title*4/3,"onSurface",{height=26}),
        kit.TextInput{id="widget-sidebar-search",width="fill",height=spacing.huge+spacing.md,text=ui.bind("widgets.search"),
            placeholder=ui.tr("bead.widgets.find"),fill="surface",border_width=1,border_color="outlineVariant",
            aria_label=ui.tr("bead.widgets.find")},
        ui.Scroll{id="widget-sidebar-scroll",width="fill",grow=1,scroll=ui.bind("widgets.nav.scroll"),ui.Column(nav)}}
end

local function detail()
    local package=registry.catalogue[selected]
    if not package then return gallery() end
    local placement=package.forms[form] and form or package.forms.bar and "bar" or "desktop"
    local group,entry,instance=form_state(selected,placement,package,selected_group)
    local bar_group,_bar_entry,_bar_instance,bar_config,bar_enabled,bar_style=form_state(selected,"bar",package,selected_group)
    local desktop_group,_desktop_entry,_desktop_instance,_desktop_config,desktop_enabled,desktop_style
    if package.forms.desktop then
        desktop_group,desktop_entry,desktop_instance,desktop_config,desktop_enabled,desktop_style=form_state(selected,"desktop",package,selected_group)
    else
        desktop_enabled=false; desktop_style=""
    end
    local instance_groups=groups_for(selected)
    local settings=active_settings(package,entry,instance)
    local desktop_available=package.forms.desktop~=nil
    local detail_width=math.max(1,width-RAIL-3*spacing.lg)
    local stage_w=math.max(1,detail_width-2*spacing.lg)
    local stage_h=math.min(stage_w*screen_h/screen_w,math.max(spacing.huge*4,height*.38))
    local stage_scale=math.min(stage_w/screen_w,stage_h/screen_h)
    local mini_w,mini_h=screen_w*stage_scale,screen_h*stage_scale
    local mini_left,mini_top=(stage_w-mini_w)/2,(stage_h-mini_h)/2
    local bar_h=math.max(1,get("bar.height",50))
    local entry_candidate=not bar_enabled and {
        widget=selected,group=bar_group,style=bar_style,settings=settings,placement=bar_config.placement,
    } or nil
    local bar_node=preview.render_bar(doc,screen_w,bar_h,stage_scale,
        bar_enabled and selected or nil,bar_style,settings,bar_group,entry_candidate)
    local stage_bar_height=bar_h*stage_scale
    local desktop_descriptor=package.forms.desktop and package.forms.desktop.styles[desktop_style]
    local design_width=desktop_descriptor and desktop_descriptor.preferred_span or 320
    local design_height=desktop_descriptor and desktop_descriptor.preferred_height or 120
    local widget_w=math.min(mini_w*.45,design_width*stage_scale)
    local widget_h=math.min(mini_h*.42,design_height*stage_scale)
    local widget_left=mini_left+(mini_w-widget_w)/2
    local widget_top=mini_top+stage_bar_height+math.max(0,(mini_h-stage_bar_height-widget_h)/2)
    local wallpaper_image
    if wallpaper~="" then
        wallpaper_image=ui.Image{id="stage-wallpaper",position="absolute",left=mini_left,top=mini_top,width=mini_w,height=mini_h,icon=wallpaper,image_fit="cover"}
    elseif wallpaper_path~="" then
        wallpaper_image=ui.Image{id="stage-wallpaper",position="absolute",left=mini_left,top=mini_top,width=mini_w,height=mini_h,icon=wallpaper_path,image_fit="cover"}
    end
    local desktop_preview=desktop_available and preview.render(selected,"desktop",desktop_style,settings,widget_w,widget_h)
    local bar_source=dragging.app_payload(doc,selected,"bar",bar_style,settings,bar_group)
    local desktop_source=desktop_available and dragging.app_payload(doc,selected,"desktop",desktop_style,settings,desktop_group)
    local settings_package=table.clone(package)
    settings_package.settings={}
    for key,schema in package.settings do
        if key~="dropdown_trigger" and key~="colour_role" and not schema.style_alias and not schema.hidden then
            settings_package.settings[key]=schema
        end
    end
    local item={iid=group,widget=selected,settings=settings}
    local fields=settings_form.rows(item,settings_package,form_action(selected,placement,group))
    local advanced_fields=settings_form.rows(item,settings_package,form_action(selected,placement,group),true)
    local colour_field=settings_form.row(item,"colour_role",package.settings.colour_role,
        settings.colour_role,form_action(selected,placement,group))
    local advanced_open=get("widgets.advanced.open",false)==true
    local stage_children={ui.Box{id="stage-background",position="absolute",left=mini_left,top=mini_top,
        width=mini_w,height=mini_h,fill="surfaceContainerHighest"}}
    if wallpaper_image then table.insert(stage_children,wallpaper_image) end
    table.insert(stage_children,ui.Box{id="stage-tint",position="absolute",left=mini_left,top=mini_top,
        width=mini_w,height=mini_h,fill=ui.alpha("surface",.08)})
        table.insert(stage_children,ui.Box{id="stage-bar",position="absolute",left=mini_left,top=mini_top,
            width=mini_w,height=stage_bar_height,clip_children=true,fill="surface",
            drag_payload={bar_source},on_drag=ui.action("widgets.drag"),bar_node})
    if desktop_preview then
        desktop_preview.props=desktop_preview.props or {}
        desktop_preview.props.metadata={{widget_entry_source=true,group=desktop_group,widget=selected,form="desktop",style=desktop_style}}
        table.insert(stage_children,ui.Box{id="desktop-widget-preview",position="absolute",left=widget_left,top=widget_top,
            width=widget_w,height=widget_h,clip_children=true,visible=true,align="center",justify="center",
            border_width=2,border_color="primary",radius=kit.tokens.radii.small,
            drag_payload={desktop_source},on_drag=ui.action("widgets.drag"),desktop_preview})
    end
    table.insert(stage_children,ui.Column{id="stage-caption",position="absolute",left=spacing.md,bottom=spacing.md,gap=spacing.xs,
        label("stage-title","Your screen",text.caption*4/3,"onSurface",{height=18}),
        label("stage-size",tostring(output_w).." × "..tostring(output_h),text.caption*4/3,"onSurfaceVariant",{height=16})})
    local stage_props={id="preview-stage",width="fill",height=stage_h,shrink=0,clip_children=true,
        radius=kit.tokens.radii.medium,fill="surfaceContainerHigh"}
    for _,child in stage_children do table.insert(stage_props,child) end
    local content_props={id="detail-content",width="fill",shrink=0,gap=spacing.md,
        ui.Column{id="colour-role-section",width="fill",shrink=0,gap=spacing.sm,padding=spacing.md,
            radius=kit.tokens.radii.medium,fill="surfaceContainer",colour_field},
        label("drag-hint",ui.tr("bead.widgets.drag-hint"),text.caption*4/3,"onSurfaceVariant",{width="fill",height=18,shrink=0}),
        ui.Box(stage_props),
        ui.Row{id="form-enable-row",width="fill",shrink=0,gap=spacing.md,
            ui.Column{id="bar-placement-actions",width=0,grow=1,gap=spacing.xs,padding=spacing.sm,
                radius=kit.tokens.radii.medium,fill="surfaceContainer",
                kit.ToggleRow{id="bar-enabled",width="fill",label=ui.tr("bead.widgets.in-bar"),checked=bar_enabled,enabled=not busy,
                    on_activate=edit({op="toggle-form",widget=selected,form="bar",enabled=not bar_enabled})},
                button("bar-placement",ui.tr(bar_enabled and "bead.widgets.remove-bar" or "bead.widgets.add-bar"),
                    edit({op="toggle-form",widget=selected,form="bar",enabled=not bar_enabled}),{width="fill",enabled=not busy})},
            desktop_available and ui.Column{id="desktop-placement-actions",width=0,grow=1,gap=spacing.xs,padding=spacing.sm,
                radius=kit.tokens.radii.medium,fill="surfaceContainer",
                kit.ToggleRow{id="desktop-enabled",width="fill",label=ui.tr("bead.widgets.on-desktop"),checked=desktop_enabled,enabled=not busy,
                    on_activate=edit({op="toggle-form",widget=selected,form="desktop",enabled=not desktop_enabled})},
                button("desktop-placement",ui.tr(desktop_enabled and "bead.widgets.remove-desktop" or "bead.widgets.add-desktop"),
                    edit({op="toggle-form",widget=selected,form="desktop",enabled=not desktop_enabled}),{width="fill",enabled=not busy})} or nil},
        ui.Column{id="dropdown-trigger",width="fill",shrink=0,gap=spacing.xs,padding=spacing.md,
            radius=kit.tokens.radii.medium,fill="surfaceContainer",
            label("trigger-label","Open its dropdown on",text.body*4/3,"onSurface",{height=22}),
            ui.Row{id="trigger-choices",width="fill",height=spacing.huge+spacing.xs,gap=spacing.xs,
                button("trigger-click","Click",edit({op="default-trigger",value="click"}),{
                    width=0,grow=1,selected=(doc.dropdown_trigger or "click")=="click"}),
                button("trigger-hover","Hover",edit({op="default-trigger",value="hover"}),{
                    width=0,grow=1,selected=doc.dropdown_trigger=="hover"})}},
        ui.Row{id="form-style-columns",width="fill",height=STYLE_SECTION_HEIGHT,shrink=0,gap=spacing.md,
            ui.Column{id="bar-style-section",width=0,grow=1,height=STYLE_SECTION_HEIGHT,shrink=0,gap=spacing.sm,padding=spacing.md,
                radius=kit.tokens.radii.medium,fill="surfaceContainer",
                label("bar-style-heading","Bar style",text.body*4/3,"onSurface",{height=22}),style_tiles(selected,"bar",package,bar_style,settings)},
            desktop_available and ui.Column{id="desktop-style-section",width=0,grow=1,height=STYLE_SECTION_HEIGHT,shrink=0,gap=spacing.sm,padding=spacing.md,
                radius=kit.tokens.radii.medium,fill="surfaceContainer",
                label("desktop-style-heading","Desktop style",text.body*4/3,"onSurface",{height=22}),style_tiles(selected,"desktop",package,desktop_style,settings)} or nil}}
    if #instance_groups>1 then
        local options={id="instance-options",width="fill",gap=spacing.sm}
        for _,copy in instance_groups do
            table.insert(options,button("instance-"..copy.id,copy.label,navigate("instance",copy.id),{
                selected=copy.id==group,fill=copy.id==group and ui.alpha("primary",.15) or "surfaceContainer"}))
        end
        table.insert(content_props,1,ui.Column{id="instance-picker",width="fill",shrink=0,gap=spacing.xs,
            label("instance-heading","Choose a copy",text.caption*4/3,"onSurfaceVariant",{height=18}),ui.Row(options)})
    end
    if next(settings_package.settings)~=nil then
        table.insert(content_props,ui.Column{id="settings-section",width="fill",shrink=0,gap=spacing.md,padding=spacing.md,
            radius=kit.tokens.radii.medium,fill="surfaceContainer",
            label("settings-title","Settings",text.title*4/3,"onSurface",{height=24}),
            #fields.children>0 and fields or nil,
            #advanced_fields.children>0 and ui.Column{id="advanced-fields",width="fill",gap=spacing.sm,
                button("advanced-toggle",advanced_open and "Hide advanced" or "Advanced",ui.action("widgets.advanced.toggle"),{fill="transparent"}),
                advanced_open and advanced_fields or nil} or nil})
    else
        local slot=instance and instance.slot or {}
        table.insert(content_props,ui.Column{id="advanced-section",width="fill",shrink=0,gap=spacing.sm,padding=spacing.md,
            radius=kit.tokens.radii.medium,fill="surfaceContainer",
            button("advanced-toggle",advanced_open and "Hide advanced" or "Advanced",ui.action("widgets.advanced.toggle"),{fill="transparent"}),
            advanced_open and ui.Grid{id="developer-fields",width="fill",columns=2,gap=spacing.md,row_height=38,
                ui.Column{id="developer-widget",gap=spacing.xs,
                    label("developer-widget-label","Widget identifier",text.caption*4/3,"onSurfaceVariant",{height=16}),
                    label("developer-widget-value",selected,text.body*4/3,"onSurface",{height=18})},
                ui.Column{id="developer-bar-style",gap=spacing.xs,
                    label("developer-bar-label","Bar style",text.caption*4/3,"onSurfaceVariant",{height=16}),
                    label("developer-bar-value",bar_style,text.body*4/3,"onSurface",{height=18})},
                ui.Column{id="developer-desktop-style",gap=spacing.xs,
                    label("developer-desktop-label","Desktop style",text.caption*4/3,"onSurfaceVariant",{height=16}),
                    label("developer-desktop-value",desktop_style,text.body*4/3,"onSurface",{height=18})},
                ui.Column{id="developer-canvas-slot",gap=spacing.xs,
                    label("developer-slot-label","Desktop position",text.caption*4/3,"onSurfaceVariant",{height=16}),
                    label("developer-slot-value",tostring(slot.x or 0)..", "..tostring(slot.y or 0),text.body*4/3,"onSurface",{height=18})}} or nil})
    end
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
        ui.Scroll{id="detail-scroll",width="fill",grow=1,scroll=ui.bind("widgets.scroll"),ui.Column(content_props)}
    }
end

local function metadata()
    return {{widget_layout=true,widget_clock_preview_seconds=selected=="clock",document=doc,initial=initial,rev=rev,catalogue=catalogue,
        screen_width=screen_w,screen_height=screen_h,snap_distance=spacing.lg,
        viewport_width=width,viewport_height=height}}
end

local app_content=ui.Column{id="widgets-app",width="100%",height="100%",fill="surface",radius=kit.tokens.radii.large,
    border_width=1,border_color=ui.alpha("outlineVariant",.55),clip_children=true,
    ui.Column{id="app-header-box",width="fill",height=65,shrink=0,
        ui.Row{id="app-header",width="fill",height=64,shrink=0,align="center",gap=spacing.lg,padding_left=spacing.xl,padding_right=spacing.xl,
            ui.Text{id="app-mark",text="widgets",family="Material Symbols Rounded",font_size=kit.tokens.icons.hero,width=44,height=44,color="primary"},
            label("app-title",ui.tr("bead.widgets.app-title"),text.title*4/3,"onSurface",{grow=1,height=30}),
            button("undo","Undo",edit({op="undo"}),{enabled=undo_available and not busy}),
            button("reset","Reset to Nacre default",edit({op="reset"}),{fill="surfaceContainerHigh",enabled=not busy})},
        ui.Box{id="header-rule",width="fill",height=1,fill="outlineVariant"}},
    ui.Row{id="app-body",width="fill",height="fill",gap=spacing.lg,padding=spacing.lg,
        widget_sidebar(),
        selected=="" and gallery() or detail()},
}
return ui.Box{id="widgets-app-root",width="100%",height="100%",metadata=metadata(),app_content}
