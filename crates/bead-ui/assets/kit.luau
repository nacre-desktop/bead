-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Framework components. Palette roles are supplied by the existing Orient palette.
return function(ui, tokens)
    local kit = {tokens=tokens, motion={color=.200, state=.120, text=.400}}
    local function value(v, fallback)
        if v == nil then return fallback end
        return ui.resolve_for_check(v)
    end
    local function props(p, defaults, skip)
        local out = table.clone(defaults or {})
        for k,v in p do if not skip or not skip[k] then out[k]=v end end
        return out
    end
    local function edge(p)
        local e=p.edge or "top"
        assert(e=="top" or e=="bottom" or e=="left" or e=="right", "edge needs top/bottom/left/right")
        return e
    end
    function kit.Button(p)
        return ui.Button(props(p,{height=40,padding=tokens.spacing.sm,radius=tokens.radii.small,
            font_size=tokens.text.body*4/3,fill="surfaceContainer",color="onSurface",hover_fill="surfaceContainerHigh"}))
    end
    function kit.IconButton(p)
        return kit.Button(props(p,{width=40,padding=tokens.spacing.xs,family="Material Symbols Rounded",font_size=tokens.icons.large*4/3}))
    end
    function kit.StatusIcon(p) return kit.Button(p) end
    function kit.Switch(p)
        local checked=value(p.checked,false)
        local out=props(p,{width=48,height=28,padding=4,radius=tokens.radii.full,
            fill=checked and "primary" or "surfaceContainerHigh",aria_label=checked and "On" or "Off"},{checked=true})
        out[1]=ui.Box{id="thumb",position="absolute",left=checked and 24 or 4,top=4,width=20,height=20,radius=10,fill=checked and "onPrimary" or "onSurface"}
        return kit.Button(out)
    end
    function kit.Slider(p)
        local lo,hi=p.min or 0,p.max or 100
        local step=p.step or (hi-lo)/20
        assert(hi>lo and step>0 and math.ceil((hi-lo)/step)<=256,"invalid slider range/step")
        local current=math.clamp(value(p.value,lo),lo,hi)
        local width=p.width or 240
        local fraction=(current-lo)/(hi-lo)
        local children={}
        local count=math.ceil((hi-lo)/step)
        for i=0,count do
            local n=math.min(hi,lo+i*step)
            children[i+1]=ui.Button{drag=true,id="value-"..i,grow=1,shrink=0,height=28,padding=0,radius=2,
                fill=ui.alpha("surface",0),aria_label=(p.label or "Value").." "..n,
                on_activate=ui.action(p.on_change,n),
                ui.Box{id="hit",width="100%",height="100%",fill=ui.alpha("surface",0)}}
        end
        return ui.Box{id=p.id,width=width,height=28,
            ui.Box{id="track",position="absolute",top=11,width="100%",height=6,radius=3,fill="surfaceContainerHigh"},
            ui.Box{id="level",position="absolute",top=11,width=(fraction*100).."%",height=6,radius=3,fill="primary"},
            ui.Box{id="thumb",position="absolute",left=fraction*(width-20),top=4,width=20,height=20,radius=10,fill="primary"},
            ui.Row(props(children,{id="hits",width="100%",height=28,gap=0}))}
    end
    function kit.Tile(p)
        return ui.Row{id=p.id,gap=tokens.spacing.sm,fill="surfaceContainer",radius=tokens.radii.medium,
            kit.Button{id="toggle",text=p.text,grow=1,on_activate=p.on_toggle},
            kit.IconButton{id="detail",text="chevron_right",aria_label="Details",on_activate=p.on_detail}}
    end
    function kit.SegmentedControl(p)
        local children={};local selected=value(p.value,"")
        for i,item in p.items do
            children[i]=kit.Button{id=item.id,text=item.label,grow=1,selected=item.id==selected,
                fill=item.id==selected and "primaryContainer" or "surfaceContainer",
                on_activate=ui.action(p.on_change,item.id)}
        end
        return ui.Row(props(children,{id=p.id,gap=tokens.spacing.xs}))
    end
    -- Keep the complete model and scroll extent; materialize viewport + one row each side.
    function kit.VirtualGrid(p)
        local columns=math.max(1,math.floor((p.width+p.gap_x)/(p.min_cell_width+p.gap_x)))
        local pitch=p.row_height+p.gap_y
        local rows=math.ceil(math.max(ui.model_count(p.model),value(p.total_count,0))/columns)
        local height=math.max(0,rows*pitch-p.gap_y)
        local scroll=math.clamp(value(p.scroll,0),0,math.max(0,height-p.height))
        local first=math.max(0,math.floor(scroll/pitch)-1)
        -- Never more than viewport + 2 rows; at the top (no row above to overscan) one row fewer.
        local count=math.min(math.ceil(p.height/pitch)+2,math.floor((scroll+p.height)/pitch)-first+2)
        return ui.Scroll{id=p.id,scroll=p.scroll,grow=1,
            ui.Box{id="extent",height=height,shrink=0,width="fill",
                ui.Grid{id="window",position="absolute",top=first*pitch,width="fill",
                    columns=columns,row_height=p.row_height,gap_x=p.gap_x,gap_y=p.gap_y,
                    ui.Repeat{id="rows",model=p.model,key=p.key,first=first*columns,count=count*columns,render=p.render},
                },
            },
        }
    end
    -- Scrolling message list over the cells `bead_ui::transcript` lays out: bubble fills,
    -- text boxes (plain, Markdown blocks, code) and per-message buttons. `cells` is the
    -- windowed model, `extent` the full height, `on_action` the action fired by buttons.
    function kit.Transcript(p)
        local out=props(p,{},{cells=true,extent=true,on_action=true})
        out[1]=ui.Box{id="extent",width=p.width,height=math.max(value(p.extent,1),1),shrink=0,
            ui.Repeat{id="cells",model=p.cells,key="id",render=function(item)
                if item.k=="fill" then
                    return ui.Box{id="fill",position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,radius=item.r,fill=item.b}
                elseif item.k=="text" then
                    return ui.Text{id="text",position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,text=item.t,
                        font_size=item.s,family=item.f,color=item.c}
                end
                local pad=math.floor((item.h-item.s*1.4)/2)
                return kit.Button{id="button",position="absolute",left=item.x,top=item.y,width=item.w,height=item.h,padding=0,padding_top=pad,
                    radius=tokens.radii.small,fill="surfaceContainerHigh",aria_label=item.t,on_activate=ui.action(p.on_action,item.a),
                    ui.Text{id="label",text=item.t,width=item.w,height=item.h-pad,font_size=item.s,text_align="center",color="onSurface"}}
            end}}
        return ui.Scroll(out)
    end
    -- Multi-line message box: Enter activates, Shift+Enter breaks the line.
    function kit.Composer(p)
        return kit.TextInput(props(p,{multiline=true,padding=tokens.spacing.md,radius=tokens.spacing.lg,fill="surfaceContainerHigh"}))
    end
    function kit.List(p) return ui.Scroll(props(p,{grow=1})) end
    function kit.ListRow(p) return kit.Button(props(p,{height=40,width="fill",text_align="left"})) end
    function kit.Card(p) return ui.Column(props(p,{padding=tokens.spacing.md,gap=tokens.spacing.sm,radius=tokens.radii.medium,fill="surfaceContainer"})) end
    function kit.TextInput(p) return ui.TextInput(props(p,{height=40,padding=tokens.spacing.sm,radius=tokens.radii.small,font_size=tokens.text.body*4/3})) end
    function kit.Bar(p)
        local e=edge(p);local vertical=e=="left" or e=="right"
        return (vertical and ui.Column or ui.Row)(props(p,{width=vertical and 50 or "100%",height=vertical and "100%" or 50,
            align="center",direction=e=="bottom" and "row-reverse" or e=="right" and "column-reverse" or vertical and "column" or "row"}))
    end
    function kit.Panel(p)
        edge(p)
        return ui.Panel(props(p,{edge="left",open=true,duration=kit.motion.color,radius=tokens.radii.medium,fill="surface",gap=tokens.spacing.sm}))
    end
    function kit.Popover(p)
        local e=edge(p);local out=props(p,{position="absolute",width=240,padding=tokens.spacing.md,
            radius=tokens.radii.medium,fill="surfaceContainerHigh"},{bar_size=true})
        local distance=(p.bar_size or 50)+tokens.spacing.sm
        if e=="top" then out.top=distance elseif e=="bottom" then out.bottom=distance
        elseif e=="left" then out.left=distance else out.right=distance end
        return ui.Column(out)
    end
    kit.Peek=kit.Popover
    function kit.Tooltip(p) return kit.Popover(props(p,{width=180,padding=tokens.spacing.sm})) end
    function kit.LevelMeter(p)
        local level=math.clamp(value(p.value,0),0,1)
        return ui.Box{id=p.id,width=p.width or 160,height=p.height or 8,radius=tokens.radii.full,fill="surfaceContainerHigh",
            ui.Box{id="level",width=(level*100).."%",height="100%",radius=tokens.radii.full,fill="primary"}}
    end
    -- Form kit (B0045). Generic rows over plain data; callers pass labels, values and
    -- ready-made actions, so no product or setting id lives here. Sizes and colours are tokens.
    local sp,tx=tokens.spacing,tokens.text
    local function maybe(action,enabled) if enabled then return action end return nil end
    function kit.SectionHeader(p)
        return ui.Column{id=p.id,width="fill",height=46,shrink=0,gap=sp.hairline,padding_top=sp.lg,
            ui.Text{id="title",text=p.label,font_size=tx.title*4/3,height=24,color="primary"},
            ui.Text{id="detail",text=p.description or "",visible=(p.description or "")~="",font_size=tx.caption*4/3,height=16,color="onSurfaceVariant"}}
    end
    function kit.HelpText(p)
        return ui.Text{id=p.id,text=p.text,width="fill",height=20,shrink=0,font_size=tx.caption*4/3,color="onSurfaceVariant"}
    end
    function kit.Swatch(p)
        return ui.Box{id=p.id,width=p.size or 24,height=p.size or 24,radius=tokens.radii.small,fill=p.color,
            border_width=1,border_color="outlineVariant",aria_label=p.label}
    end
    -- Label and description on the left, the control children on the right.
    function kit.FormRow(p)
        local on=p.enabled~=false
        local out=props(p,{width="fill",height=p.height or 56,shrink=0,gap=sp.md,align="center",padding_left=sp.sm,padding_right=sp.sm,
            radius=tokens.radii.small},{label=true,description=true,enabled=true,height=true})
        local labels=ui.Column{id="labels",grow=1,gap=sp.hairline,justify="center",
            ui.Text{id="label",text=p.label,font_size=tx.body*4/3,height=20,color=on and "onSurface" or "onSurfaceVariant"},
            ui.Text{id="description",text=p.description or "",visible=(p.description or "")~="",font_size=tx.caption*4/3,height=16,color="onSurfaceVariant"}}
        local row={labels}
        for _,child in ipairs(p) do table.insert(row,child) end
        for k,v in pairs(out) do if type(k)~="number" then row[k]=v end end
        return ui.Row(row)
    end
    function kit.ToggleRow(p)
        return kit.FormRow{id=p.id,label=p.label,description=p.description,enabled=p.enabled,
            kit.Switch{id="switch",checked=p.checked,aria_label=p.label,on_activate=maybe(p.on_activate,p.enabled)}}
    end
    function kit.ChoiceRow(p)
        local on=p.enabled~=false
        local buttons={}
        for i,o in ipairs(p.options) do
            buttons[i]=kit.Button{id="option-"..i,text=o.label,height=32,padding=sp.xs,padding_left=sp.sm,padding_right=sp.sm,shrink=0,
                width=sp.xxl+(utf8.len(o.label) or 4)*8,selected=o.selected,
                fill=o.selected and "primaryContainer" or "surfaceContainer",color=o.selected and "onPrimaryContainer" or "onSurface",
                aria_label=p.label..": "..o.label,on_activate=maybe(ui.action(p.on_change,o.id),on)}
        end
        buttons.id="options";buttons.gap=sp.xs
        return kit.FormRow{id=p.id,label=p.label,description=p.description,enabled=p.enabled,ui.Row(buttons)}
    end
    function kit.NumberRow(p)
        local on=p.enabled~=false
        return kit.FormRow{id=p.id,label=p.label,description=p.description,enabled=p.enabled,
            kit.IconButton{id="decrease",text="remove",aria_label=p.label.." -",on_activate=maybe(p.on_decrease,on and not p.at_min)},
            ui.Text{id="value",text=p.text..(p.unit or ""),width=64,height=24,text_align="center",font_size=tx.body*4/3,color="onSurface"},
            kit.IconButton{id="increase",text="add",aria_label=p.label.." +",on_activate=maybe(p.on_increase,on and not p.at_max)}}
    end
    -- Value as a button; editing shows the shared text field with save and cancel.
    function kit.TextRow(p)
        local on=p.enabled~=false
        if p.editing then
            return kit.FormRow{id=p.id,label=p.label,description=p.description,enabled=p.enabled,
                kit.TextInput{id="field",text=p.field,width=220,aria_label=p.label},
                kit.IconButton{id="save",text="check",aria_label=p.save_label,on_activate=p.on_save},
                kit.IconButton{id="cancel",text="close",aria_label=p.cancel_label,on_activate=p.on_cancel}}
        end
        local children={}
        if p.swatch then table.insert(children,kit.Swatch{id="swatch",color=p.swatch,label=p.label}) end
        table.insert(children,kit.Button{id="edit",text=p.text,width=220,height=32,padding=sp.xs,padding_left=sp.sm,aria_label=p.label..": "..p.text,
            on_activate=maybe(p.on_edit,on)})
        return kit.FormRow{id=p.id,label=p.label,description=p.description,enabled=p.enabled,table.unpack(children)}
    end
    function kit.ListRow(p)
        local on=p.enabled~=false
        local rows={}
        for i,item in ipairs(p.items) do
            rows[i]=ui.Row{id="item-"..i,width="fill",height=32,shrink=0,gap=sp.sm,align="center",
                ui.Text{id="text",text=item.label,grow=1,height=20,font_size=tx.body*4/3,color="onSurface"},
                kit.IconButton{id="remove",width=32,height=32,shrink=0,text="close",aria_label=p.remove_label.." "..item.label,on_activate=maybe(ui.action(p.on_remove,(p.remove_prefix or "")..item.id),on)}}
        end
        rows.id="items";rows.width="fill";rows.gap=sp.xs;rows.shrink=0
        local add
        if p.editing then
            add=ui.Row{id="add",width="fill",gap=sp.sm,align="center",
                kit.TextInput{id="field",text=p.field,grow=1,aria_label=p.label},
                kit.IconButton{id="save",text="check",aria_label=p.save_label,on_activate=p.on_save},
                kit.IconButton{id="cancel",text="close",aria_label=p.cancel_label,on_activate=p.on_cancel}}
        else
            add=kit.Button{id="add",text=p.add_label,height=32,width=120,padding=sp.xs,padding_left=sp.sm,on_activate=maybe(p.on_edit,on)}
        end
        return ui.Column{id=p.id,width="fill",shrink=0,gap=sp.xs,padding=sp.sm,
            ui.Text{id="label",text=p.label,height=20,font_size=tx.body*4/3,color=on and "onSurface" or "onSurfaceVariant"},
            ui.Column(rows),add}
    end
    -- A choice that needs an explicit apply: shows what is pending and offers both ways out.
    function kit.ApplyBar(p)
        return ui.Row{id=p.id,width="fill",height=40,shrink=0,gap=sp.sm,align="center",padding_left=sp.sm,radius=tokens.radii.small,fill="secondaryContainer",
            ui.Text{id="pending",text=p.text,grow=1,height=20,font_size=tx.caption*4/3,color="onSecondaryContainer"},
            kit.Button{id="apply",text=p.apply_label,width=96,on_activate=p.on_apply},
            kit.Button{id="discard",text=p.discard_label,width=96,on_activate=p.on_discard}}
    end
    function kit.BlurredArtwork(p) return ui.Image(props(p,{blur=12})) end
    return kit
end
