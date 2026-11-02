-- SPDX-License-Identifier: MIT
-- Drag payload, tree attachment and animation geometry. Host identities and actions are callbacks.
return function()
    local M={}
    local prepared_renders=setmetatable({}, {__mode="k"})

    function M.document_value(value,hosts_key)
        hosts_key=hosts_key or "hosts"
        if type(value)~="table" then return nil end
        if type(value[1])=="table" and type(value[1][hosts_key])=="table" then return value[1] end
        if type(value[hosts_key])=="table" then return value end
    end

    function M.group_state(document,item)
        local group=item.group or item.iid
        for name,entry in document.widgets or {} do
            if entry.widget==item.widget and (name==group or not group) then return name,entry end
        end
        return group,document.widgets and document.widgets[group] or nil
    end

    function M.settings_for(package,state,item)
        local values=table.clone(package.defaults)
        for key,value in state and state.settings or {} do values[key]=value end
        for key,value in item.settings or {} do values[key]=value end
        return values
    end

    function M.transfer_settings(package,values)
        local result={}
        for key in package.settings do if values[key]~=nil then result[key]=values[key] end end
        return result
    end

    function M.form_for(document,group,state,item,form,package,host_names)
        local config=state and state.forms and state.forms[form] or {}
        local canvas_host=assert(host_names and host_names.canvas,"host must supply its canvas identity")
        local current=(item.host==canvas_host and "desktop" or "bar")==form
        local style=config.style
        if (style==nil or style=="") and current then style=item.style end
        if style==nil or style=="" then
            local alias=package.forms[form].legacy_style_setting
            style=alias and M.settings_for(package,state,item)[alias] or package.forms[form].default_style
        end
        return config,style
    end

    function M.payload(document,item,options)
        local source=options.frozen_source and options.frozen_source(item.iid)
        if source then return source end
        local package=options.catalogue[item.widget]
        if not package then return nil end
        local host_name=options.host_name(item)
        local form=options.form_name(item)
        local group,state=M.group_state(document,item)
        local _,style=M.form_for(document,group,state,item,form,package,options.host_names)
        return {iid=item.iid,group=group,widget=item.widget,host=host_name,form=form,style=style,
            settings=M.transfer_settings(package,M.settings_for(package,state,item))}
    end

    function M.app_payload(document,widget,form,style,settings,selected_group,options)
        local package=options.catalogue[widget]
        if not package or not package.forms[form] then return nil end
        local group,state=M.group_state(document,{widget=widget,group=selected_group})
        local values=table.clone(package.defaults)
        for key,value in state and state.settings or {} do values[key]=value end
        for key,value in settings or {} do values[key]=value end
        if style==nil or style=="" then style=package.forms[form].default_style end
        return {group=group,widget=widget,host=options.host_for_form(form),form=form,
            style=style,settings=M.transfer_settings(package,values)}
    end

    function M.group_drag_target(node)
        local marked,interactive={},{}
        local function scan(current)
            if type(current)~="table" then return end
            local props=current.props
            if type(props)=="table" then
                if props.drag_payload~=nil then table.insert(marked,props) end
                if props.on_activate~=nil then table.insert(interactive,props) end
            end
            for _,child in current.children or {} do scan(child) end
        end
        scan(node)
        if #marked==1 then return marked[1] end
        if #marked>1 then return nil end
        if #interactive==1 then return interactive[1] end
    end

    function M.prepare_node(node,alpha,options)
        if type(node)~="table" then return end
        local props=node.props
        if type(props)=="table" then
            local kind=type(node.kind)=="string" and string.lower(node.kind :: string) or ""
            if kind=="button" and props.on_activate~=nil then
                props.activate_on_release=true
                props.focus_on_pointer=false
            end
            if alpha~=nil then
                for _,key in options.color_fields do
                    local value=props[key]
                    if (type(value)=="string" and value~="transparent")
                        or (type(value)=="table" and (value.role~=nil or value.base~=nil)) then
                        props[key]=options.alpha(value,alpha)
                    end
                end
            end
            if type(props.render)=="function" then
                local render=props.render
                local variants=prepared_renders[render]
                if not variants then variants=setmetatable({}, {__mode="v"}); prepared_renders[render]=variants end
                local mode=alpha==nil and "normal" or "alpha:"..tostring(alpha)
                local wrapper=variants[mode]
                if not wrapper then
                    -- A retained render wrapper needs only paint policy, not the
                    -- per-instance document/resize callbacks in attachment options.
                    local paint={color_fields=options.color_fields,alpha=options.alpha}
                    wrapper=function(...)
                        local generated=render(...)
                        M.prepare_node(generated,alpha,paint)
                        return generated
                    end
                    variants[mode]=wrapper
                end
                props.render=wrapper
            end
        end
        for _,child in node.children or {} do M.prepare_node(child,alpha,options) end
    end

    function M.attach_tree(item,node,payload,options)
        if not node or type(node.props)~="table" then return node end
        if payload then
            if node.kind=="group" then
                local target=M.group_drag_target(node)
                if target then
                    target.drag_payload={payload}
                    target.on_drag=options.drag_action
                elseif type(node.props.width)=="number" and node.props.width>0
                    and type(node.props.height)=="number" and node.props.height>0 then
                    local props={id=options.wrapper_id(item),drag_payload={payload},on_drag=options.drag_action,
                        width=node.props.width,height=node.props.height,shrink=node.props.shrink}
                    table.insert(props,node)
                    node=options.wrap(props)
                end
            else
                node.props.drag_payload={payload}
                node.props.on_drag=options.drag_action
            end
        end
        local marker=node.kind=="group" and M.group_drag_target(node) or node.props
        if marker then marker.metadata=options.metadata(item) end
        M.prepare_node(node,options.alpha_for(item),options)
        local handle=options.resize_handle(item)
        if handle then table.insert(node.children,handle) end
        return node
    end

    function M.ghost_geometry(args)
        local preparing=args.phase==args.prepare_phase
        local animated_width=args.animate(args.width_key,preparing and args.source_width or args.target_width,
            preparing and 0 or args.size_duration,args.size_easing)
        local animated_height=args.animate(args.height_key,preparing and args.source_height or args.target_height,
            preparing and 0 or args.size_duration,args.size_easing)
        local width=preparing and args.target_width or animated_width
        local height=preparing and args.target_height or animated_height
        local dx_target=-args.grab_x*(animated_width/args.source_width)
        local dy_target=-args.grab_y*(animated_height/args.source_height)
        if args.phase==args.settle_phase or args.phase==args.return_phase then
            dx_target=(args.snap_x or args.pointer_x+dx_target)-args.pointer_x
            dy_target=(args.snap_y or args.pointer_y+dy_target)-args.pointer_y
        end
        local settling=args.phase==args.settle_phase or args.phase==args.return_phase
        local left=args.pointer_x+args.animate(args.x_key,dx_target,settling and args.settle_duration or 0,args.settle_easing)
        local top=args.pointer_y+args.animate(args.y_key,dy_target,settling and args.settle_duration or 0,args.settle_easing)
        return {width=width,height=height,left=left,top=top}
    end

    return M
end
