-- SPDX-License-Identifier: GPL-3.0-only
-- The Widgets app is a catalogue first; placement and style edits stay in the page.
local context=require("../context")
local ui,kit=context.ui,context.kit
local spacing,text=kit.tokens.spacing,kit.tokens.text
local registry=require("../registry")
local preview=require("preview")
local settings_form=require("../settings-form")

local function catalogue_descriptors()
    local packages_out={}
    for _,package in registry.catalogue do
        local descriptor={id=package.id,title=package.title,icon=package.icon,category=package.category,
            defaults=package.defaults,settings=package.settings,forms={}}
        for placement,form_def in package.forms do
            if placement~="panel" then
                local form_copy={default_style=form_def.default_style,legacy_style_setting=form_def.legacy_style_setting,styles={}}
                for name,style in form_def.styles do
                    form_copy.styles[name]={title=style.title,min_span=style.min_span,max_span=style.max_span,preferred_span=style.preferred_span,preferred_height=style.preferred_height or spacing.huge*3}
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
local width=get("bead.viewport.width",1280)
local height=get("bead.viewport.height",800)
local screen_w=math.max(1,get("widgets.screen_width",1920))
local screen_h=math.max(1,get("widgets.screen_height",1080))
local doc=document_value(get("widgets.draft")) or document_value(get("widgets.layout"))
    or document_value(get("widgets.initial")) or registry.default(screen_w)
local initial=document_value(get("widgets.initial")) or registry.default(screen_w)
registry.validate(doc)
local rev=get("widgets.rev",0)
local catalogue=get("widgets.catalogue",{})
if #catalogue==0 then catalogue=catalogue_descriptors() end
local selected=get("widgets.selected","")
local category=get("widgets.category","all")
local searching=get("widgets.search","")
local form=get("widgets.form","bar")
local undo_state=get("widgets.undo",false)
local undo_available=(type(undo_state)=="table" and #undo_state>0) or undo_state==true
local message=get("widgets.message","")
local error_message=get("widgets.error","")
local busy=get("widgets.busy",false)
local wallpaper=get("widgets.wallpaper","")

local function navigate(op,value)
    return ui.action("widgets.navigate",{{op=op,value=value,document=doc,initial=initial,rev=rev,catalogue=catalogue}})
end
local function edit(request)
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
    {id="all",label="All"},
    {id="bar",label="Bar essentials"},
    {id="status",label="Status"},
    {id="media",label="Media"},
    {id="system",label="System"},
    {id="containers",label="Containers"},
    {id="yours",label="Your widgets"},
}
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
        table.insert(rows,{id=id,title=package.title,icon=package.icon,category=package.category})
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
local gallery_grid_width=math.max(1,width-2*spacing.lg-RAIL-spacing.xl)
local CARD_W=math.max(160,(gallery_grid_width-spacing.md)/2)
local PREVIEW_W=math.max(48,CARD_W-2*spacing.md-50-spacing.sm)
local function gallery_card(item)
    local package=registry.catalogue[item.id]
    local bar=preview.render(item.id,"bar",style_name(package,"bar"),package.defaults,PREVIEW_W,36)
    local desktop=package.forms.desktop and preview.render(item.id,"desktop",style_name(package,"desktop"),package.defaults,PREVIEW_W,48)
    return ui.Button{id="widget-card-"..item.id,width="fill",height=208,shrink=0,padding=spacing.md,
        radius=kit.tokens.radii.medium,fill="surfaceContainer",hover_fill="surfaceContainerHigh",
        selected=selected==item.id,selected_fill=ui.alpha("primary",.12),focus_on_pointer=false,
        aria_label="Open "..item.title,on_activate=navigate("select",item.id),
        ui.Column{id="card-content",width="fill",height="fill",gap=spacing.xs,
            ui.Row{id="card-heading",width="fill",height=30,align="center",gap=spacing.sm,
                ui.Text{id="card-icon",text=item.icon or "widgets",family="Material Symbols Rounded",font_size=kit.tokens.icons.medium,width=24,height=24,color="primary"},
                label("card-title",item.title,text.title*4/3,"onSurface",{grow=1,height=24}),
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
    return ui.Button{id="category-"..item.id,width="fill",height=spacing.huge+spacing.sm,padding_left=spacing.md,
        radius=kit.tokens.radii.small,fill=active and ui.alpha("primary",.15) or "transparent",
        hover_fill="surfaceContainerHigh",selected=active,focus_on_pointer=false,
        aria_label=item.label,on_activate=navigate("category",item.id),
        ui.Row{id="category-row",width="fill",height="fill",align="center",gap=spacing.sm,
            ui.Text{id="category-mark",text=active and "radio_button_checked" or "radio_button_unchecked",
                family="Material Symbols Rounded",font_size=kit.tokens.icons.small,width=18,height=18,color=active and "primary" or "onSurfaceVariant"},
            label("category-label",item.label,text.body*4/3,active and "onSurface" or "onSurfaceVariant",{grow=1,height=20}),
        }}
end

local function gallery()
    local rows=visible_packages()
    local category_nodes={id="category-list",width="fill",gap=spacing.xs}
    for _,item in categories do table.insert(category_nodes,category_button(item)) end
    local card_nodes={id="widget-grid",width="fill",columns=2,gap=spacing.md,row_height=220}
    for _,item in rows do table.insert(card_nodes,gallery_card(item)) end
    return ui.Row{id="gallery-page",width="fill",height="fill",padding=spacing.lg,gap=spacing.xl,
        ui.Column{id="gallery-rail",width=RAIL,height="fill",shrink=0,padding=spacing.md,gap=spacing.sm,
            radius=kit.tokens.radii.medium,fill="surfaceContainer",
            kit.TextInput{id="gallery-search",width="fill",height=spacing.huge+spacing.md,text=ui.bind("widgets.search"),
                placeholder="Find a widget",fill="surface",border_width=1,border_color="outlineVariant",aria_label="Find a widget"},
            ui.Scroll{id="category-scroll",width="fill",grow=1,scroll=ui.bind("widgets.nav.scroll"),
                ui.Column(category_nodes)},
            ui.Column{id="dropdown-default",width="fill",shrink=0,gap=spacing.sm,
                label("trigger-label","Default dropdown trigger",text.caption*4/3,"onSurfaceVariant",{height=20}),
                ui.Row{id="trigger-choices",width="fill",height=spacing.huge+spacing.xs,gap=spacing.sm,
                    button("trigger-click","Click",edit({op="default-trigger",value="click"}),{
                        width=(RAIL-2*spacing.md-spacing.sm)/2,selected=(doc.dropdown_trigger or "click")=="click"}),
                    button("trigger-hover","Hover",edit({op="default-trigger",value="hover"}),{
                        width=(RAIL-2*spacing.md-spacing.sm)/2,selected=doc.dropdown_trigger=="hover"})}}},
        ui.Column{id="gallery-content",width="fill",height="fill",grow=1,gap=spacing.md,
            ui.Row{id="gallery-title-row",width="fill",height=54,shrink=0,align="center",
                ui.Column{id="gallery-title-copy",width=0,grow=1,gap=spacing.xs,
                    label("gallery-title","Choose a widget",text.headline*4/3,"onSurface",{width="fill",height=28}),
                    label("gallery-subtitle","Preview a style, then choose where it belongs.",text.caption*4/3,"onSurfaceVariant",{width="fill",height=18})},
                label("widget-count",tostring(#rows).." widgets",text.caption*4/3,"onSurfaceVariant",{height=18})},
            #rows==0 and ui.Box{id="gallery-empty",grow=1,width="fill",radius=kit.tokens.radii.medium,fill="surfaceContainer",
                label("gallery-empty-text","No widgets match this search.",text.body*4/3,"onSurfaceVariant",{width="fill",height=24,text_align="center"})}
                or ui.Scroll{id="gallery-scroll",width="fill",grow=1,scroll=ui.bind("widgets.scroll"),
                    ui.Grid(card_nodes)}}
    }
end

local function find_item(widget)
    local found
    local function scan(items)
        for _,item in items or {} do
            if item.widget==widget then found=item; return end
            scan(item.children)
            if found then return end
        end
    end
    for _,host in doc.hosts or {} do scan(host.instances) end
    return found
end
local function config_for(widget)
    local instance=find_item(widget)
    for group,entry in doc.widgets or {} do
        if entry.widget==widget then return group,entry,instance end
    end
    local group=instance and (instance.group or instance.iid) or widget
    return group,{widget=widget,settings=instance and instance.settings or {},forms={}},instance
end
local function form_state(widget,placement,package)
    local group,entry,instance=config_for(widget)
    local config=entry.forms and entry.forms[placement] or {}
    local exists=false
    local function scan(items)
        for _,item in items or {} do
            if item.widget==widget then exists=true; return end
            scan(item.children)
            if exists then return end
        end
    end
    local host=placement=="bar" and doc.hosts.top_bar or doc.hosts.canvas
    scan(host and host.instances)
    local active=exists
    if config.enabled~=nil then active=config.enabled end
    local configured=config.style
    if configured==nil or configured=="" then
        configured=nil
        local function matching(items)
            for _,item in items or {} do
                if item.widget==widget then configured=item.style or (package.forms[placement].legacy_style_setting and (item.settings or {})[package.forms[placement].legacy_style_setting]); return true end
                if matching(item.children) then return true end
            end
            return false
        end
        matching(host and host.instances)
    end
    return group,entry,instance,config,active,configured or style_name(package,placement)
end
local function active_settings(package,entry,instance)
    local values=table.clone(package.defaults)
    for key,value in entry.settings or instance and instance.settings or {} do values[key]=value end
    return values
end
local function form_action(widget,placement)
    return function(op,fields)
        local request={op=op,widget=widget,form=placement}
        for key,value in fields or {} do
            if key~="iid" then request[key]=value end
        end
        return edit(request)
    end
end
local function style_tile(widget,placement,package,style_id,style,settings,is_selected)
    local count=0
    for _ in package.forms[placement].styles do count+=1 end
    local columns=math.max(1,math.min(4,count))
    local preview_width=math.max(76,math.min(240,(width-2*spacing.lg-(columns-1)*spacing.sm)/columns-2*spacing.xs))
    local tile_preview=preview.render(widget,placement,style_id,settings,preview_width,44)
    return ui.Button{id="style-"..style_id,width="fill",height=104,shrink=0,padding=spacing.xs,
        radius=kit.tokens.radii.small,fill=is_selected and ui.alpha("primary",.15) or "surfaceContainer",
        hover_fill="surfaceContainerHigh",selected=is_selected,focus_on_pointer=false,
        aria_label=style.title,on_activate=edit({op="style",widget=widget,form=placement,style=style_id}),
        ui.Column{id="style-card",width="fill",height="fill",gap=spacing.xs,align="center",
            ui.Box{id="style-preview",width="fill",height=54,align="center",justify="center",
                radius=kit.tokens.radii.small,fill="surfaceContainerHighest",clip_children=true,tile_preview},
            label("style-title",style.title,text.caption*4/3,"onSurface",{width="fill",height=18,text_align="center"})}}
end
local function style_tiles(widget,placement,package,style_id,settings)
    local forms=package.forms[placement]
    local keys={}
    for key in forms.styles do table.insert(keys,key) end
    table.sort(keys)
    local tiles={id="style-tiles",width="fill",columns=math.max(1,math.min(4,#keys)),gap=spacing.sm,row_height=104,shrink=0}
    for _,key in keys do table.insert(tiles,style_tile(widget,placement,package,key,forms.styles[key],settings,key==style_id)) end
    return ui.Grid(tiles)
end

local function detail()
    local package=registry.catalogue[selected]
    if not package then return gallery() end
    local placement=package.forms[form] and form or package.forms.bar and "bar" or "desktop"
    local group,entry,instance,_config,enabled,style_id=form_state(selected,placement,package)
    local settings=active_settings(package,entry,instance)
    local desktop_available=package.forms.desktop~=nil
    local stage_w=math.max(1,width-2*spacing.lg)
    local stage_h=math.clamp(height*.38,spacing.huge*4,spacing.huge*7)
    local stage_scale=math.min(stage_w/screen_w,stage_h/screen_h)
    local mini_w,mini_h=screen_w*stage_scale,screen_h*stage_scale
    local mini_left,mini_top=(stage_w-mini_w)/2,(stage_h-mini_h)/2
    local bar_h=math.max(1,get("bar.height",50))
    local bar_node=preview.render_bar(doc,screen_w,bar_h,stage_scale,
        placement=="bar" and enabled and selected or nil,style_id,settings)
    local stage_bar_height=bar_h*stage_scale
    local widget_w=math.min(mini_w*.38,320*stage_scale)
    local widget_h=math.min(widget_w*.5,mini_h*.4)
    local widget_left=mini_left+(mini_w-widget_w)/2
    local widget_top=mini_top+stage_bar_height+math.max(0,(mini_h-stage_bar_height-widget_h)/2)
    local wallpaper_image=wallpaper~="" and ui.Image{id="stage-wallpaper",position="absolute",left=mini_left,top=mini_top,width=mini_w,height=mini_h,icon=wallpaper,image_fit="cover"} or nil
    local desktop_preview
    if placement=="desktop" and enabled then
        desktop_preview=preview.render(selected,"desktop",style_id,settings,widget_w,widget_h)
    end
    local settings_package=table.clone(package)
    settings_package.settings={}
    for key,schema in package.settings do
        if not schema.style_alias and not schema.hidden then settings_package.settings[key]=schema end
    end
    local item={iid=group,widget=selected,settings=settings}
    local fields=settings_form.rows(item,settings_package,form_action(selected,placement))
    local stage_children={ui.Box{id="stage-background",position="absolute",left=mini_left,top=mini_top,
        width=mini_w,height=mini_h,fill="surfaceContainerHighest"}}
    if wallpaper_image then table.insert(stage_children,wallpaper_image) end
    table.insert(stage_children,ui.Box{id="stage-tint",position="absolute",left=mini_left,top=mini_top,
        width=mini_w,height=mini_h,fill=ui.alpha("surface",.08)})
    table.insert(stage_children,ui.Box{id="stage-bar",position="absolute",left=mini_left,top=mini_top,
        width=mini_w,height=stage_bar_height,clip_children=true,fill="surface",bar_node})
    if desktop_preview then
        table.insert(stage_children,ui.Box{id="desktop-widget-preview",position="absolute",left=widget_left,top=widget_top,
            width=widget_w,height=widget_h,clip_children=true,visible=true,align="center",justify="center",desktop_preview})
    end
    table.insert(stage_children,ui.Column{id="stage-caption",position="absolute",left=spacing.md,bottom=spacing.md,gap=spacing.xs,
        label("stage-title","Your screen",text.caption*4/3,"onSurface",{height=18}),
        label("stage-size",tostring(screen_w).." × "..tostring(screen_h),text.caption*4/3,"onSurfaceVariant",{height=16})})
    local stage_props={id="preview-stage",width="fill",height=stage_h,shrink=0,clip_children=true,
        radius=kit.tokens.radii.medium,fill="surfaceContainerHigh"}
    for _,child in stage_children do table.insert(stage_props,child) end
    local tabs_props={id="form-tabs",width="fill",height=46,shrink=0,gap=spacing.xs,
        button("bar-tab","In the bar",navigate("form","bar"),{selected=placement=="bar",fill=placement=="bar" and ui.alpha("primary",.15) or "surfaceContainer"})}
    if desktop_available then
        table.insert(tabs_props,button("desktop-tab","On the desktop",navigate("form","desktop"),
            {selected=placement=="desktop",fill=placement=="desktop" and ui.alpha("primary",.15) or "surfaceContainer"}))
    end
    local content_props={id="detail-content",width="fill",gap=spacing.md,
        ui.Box(stage_props),
        ui.Row(tabs_props),
        kit.ToggleRow{id="form-enabled",label=placement=="bar" and "Show in the bar" or "Show on the desktop",
            checked=enabled,enabled=not busy,on_activate=edit({op="toggle-form",widget=selected,form=placement,enabled=not enabled})},
        label("style-heading","Style",text.body*4/3,"onSurface",{height=22}),
        style_tiles(selected,placement,package,style_id,settings)}
    if next(settings_package.settings)~=nil then
        table.insert(content_props,ui.Column{id="settings-section",width="fill",gap=spacing.md,padding=spacing.md,
            radius=kit.tokens.radii.medium,fill="surfaceContainer",
            label("settings-title","Settings",text.title*4/3,"onSurface",{height=24}),fields})
    end
    if error_message~="" or message~="" then
        table.insert(content_props,label("save-status",error_message~="" and error_message or message,
            text.caption*4/3,error_message~="" and "error" or "onSurfaceVariant",{width="fill",height=20}))
    end
    return ui.Column{id="widget-detail",width="fill",height="fill",gap=spacing.md,padding=spacing.lg,
        ui.Row{id="detail-heading",width="fill",height=54,shrink=0,gap=spacing.md,align="center",
            button("back","arrow_back",navigate("back",selected),{width=spacing.huge+spacing.md,fill="transparent",font_size=kit.tokens.icons.medium,family="Material Symbols Rounded"}),
            ui.Text{id="detail-icon",text=package.icon or "widgets",family="Material Symbols Rounded",font_size=kit.tokens.icons.large,width=36,height=36,color="primary"},
            ui.Column{id="detail-title-copy",width=0,grow=1,gap=spacing.xs,
                label("detail-title",package.title,text.headline*4/3,"onSurface",{width="fill",height=28}),
                label("detail-description","Choose a style for each place this widget can appear.",text.caption*4/3,"onSurfaceVariant",{height=18})}},
        ui.Scroll{id="detail-scroll",width="fill",grow=1,scroll=ui.bind("widgets.scroll"),ui.Column(content_props)}
    }
end

local function metadata()
    return {{widget_layout=true,widget_clock_preview_seconds=selected=="clock",document=doc,initial=initial,rev=rev,catalogue=catalogue,
        screen_width=screen_w,screen_height=screen_h}}
end

local app_content=ui.Column{id="widgets-app",width="100%",height="100%",fill="surface",
    ui.Column{id="app-header-box",width="fill",height=65,shrink=0,
        ui.Row{id="app-header",width="fill",height=64,shrink=0,align="center",gap=spacing.lg,padding_left=spacing.xl,padding_right=spacing.xl,
            ui.Text{id="app-mark",text="widgets",family="Material Symbols Rounded",font_size=kit.tokens.icons.hero,width=44,height=44,color="primary"},
            label("app-title","Widgets",text.title*4/3,"onSurface",{grow=1,height=30}),
            button("undo","Undo",edit({op="undo"}),{enabled=undo_available and not busy}),
            button("reset","Reset to Nacre default",edit({op="reset"}),{fill="surfaceContainerHigh",enabled=not busy})},
        ui.Box{id="header-rule",width="fill",height=1,fill="outlineVariant"}},
    selected=="" and gallery() or detail(),
}
return ui.Box{id="widgets-app-root",width="100%",height="100%",metadata=metadata(),app_content}
