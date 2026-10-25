-- SPDX-License-Identifier: GPL-3.0-only
-- Reusable schema-driven widget controls; composition and action routing belong to the app.
local context=require("context")
local ui,kit=context.ui,context.kit
local sp,tx=kit.tokens.spacing,kit.tokens.text
local M={}

local function sorted_keys(values)
    local keys={}
    for key in values do table.insert(keys,key) end
    table.sort(keys)
    return keys
end

local function wire_control(node,dummy,action_factory,item,key)
    if type(node)~="table" then return end
    local props=node.props or node
    local action=props.on_activate
    if type(action)=="table" and action.name==dummy then
        props.on_activate=action_factory("setting",{iid=item.iid,key=key,value=action.argument})
        props.focus_on_pointer=false
        if props.pointer_arg~=nil then
            props.pointer_arg=action_factory("setting",{iid=item.iid,key=key,value=props.pointer_arg}).argument
        end
    end
    for _,child in node.children or {} do wire_control(child,dummy,action_factory,item,key) end
end

local function text(id,value,height,size,color)
    local font_size=size or tx.body*4/3
    return ui.Text{id=id,text=value or "",width=math.ceil(ui.text_width(value or "","IBM Plex Sans",font_size))+sp.hairline,
        height=height or 20,font_size=font_size,color=color or "onSurface"}
end

local function button(id,label,action,width)
    return kit.Button{id=id,text=label,width=width or math.ceil(ui.text_width(label,"IBM Plex Sans",tx.body*4/3))+sp.lg,
        height=sp.huge+sp.xs,shrink=0,padding=sp.xs,focus_on_pointer=false,on_activate=action}
end

function M.row(item,key,schema,value,action_factory)
    local label=key:gsub("_"," ")
    if schema.type=="toggle" then
        return kit.ToggleRow{id="field-"..key,label=label,checked=value==true,enabled=true,
            on_activate=action_factory("setting",{iid=item.iid,key=key,value=not value})}
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
                on_toggle=action_factory("menu",{key=key}),width=sp.huge*9,on_change="widgets.choice"}
        end
        wire_control(control,"widgets.choice",action_factory,item,key)
        if #options<=3 then
            return ui.Column{id="choice-"..key,width="fill",shrink=0,gap=sp.xs,text("label",label),control}
        end
        return control
    elseif schema.type=="slider" then
        local control=kit.Slider{id="slider-"..key,label=label,value=value or schema.min,min=schema.min,max=schema.max,
            step=schema.step or (schema.max-schema.min)/20,width=sp.huge*9,on_change="widgets.choice"}
        wire_control(control,"widgets.choice",action_factory,item,key)
        return control
    elseif schema.type=="number" then
        local step=schema.step or ((schema.max-schema.min)/10)
        return ui.Row{id="number-"..key,width="fill",height=sp.huge+sp.sm,align="center",gap=sp.xs,
            text("label",label,22,tx.body),
            button("minus","−",action_factory("setting",{iid=item.iid,key=key,value=math.max(schema.min,(value or schema.min)-step)}),40),
            text("value",tostring(value or schema.min),22,tx.body),
            button("plus","+",action_factory("setting",{iid=item.iid,key=key,value=math.min(schema.max,(value or schema.min)+step)}),40)}
    elseif schema.type=="text" or schema.type=="color" or schema.type=="icon" then
        local editing=ui.resolve_for_check(ui.bind("widgets.select"))==key
        return kit.TextRow{id="field-"..key,label=label,
            text=(value~=nil and tostring(value)~="") and tostring(value) or "Set value",editing=editing,enabled=true,
            field=ui.bind("widgets.field"),on_edit=action_factory("field",{iid=item.iid,key=key,value=value or ""}),
            on_save=action_factory("setting",{iid=item.iid,key=key,from_field=true}),
            on_cancel=action_factory("select",{iid=item.iid})}
    end
end

function M.rows(item,package,action_factory)
    local rows={id="settings-fields",width="fill",gap=sp.sm,shrink=0}
    for _,key in sorted_keys(package.settings) do
        local schema=package.settings[key]
        if schema.hidden then continue end
        local value=(item.settings or {})[key]
        if value==nil then value=package.defaults[key] end
        local row=M.row(item,key,schema,value,action_factory)
        if row then table.insert(rows,row) end
    end
    return ui.Column(rows)
end

return M
