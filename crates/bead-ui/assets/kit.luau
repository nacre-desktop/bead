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
        local model=value(p.model,{})
        local rows=math.ceil(#model/columns)
        local height=math.max(0,rows*pitch-p.gap_y)
        local scroll=math.clamp(value(p.scroll,0),0,math.max(0,height-p.height))
        local first=math.max(0,math.floor(scroll/pitch)-1)
        local count=math.ceil(p.height/pitch)+2
        return ui.Scroll{id=p.id,scroll=p.scroll,grow=1,
            ui.Box{id="extent",height=height,shrink=0,width="fill",
                ui.Grid{id="window",position="absolute",top=first*pitch,width="fill",
                    columns=columns,row_height=p.row_height,gap_x=p.gap_x,gap_y=p.gap_y,
                    ui.Repeat{id="rows",model=p.model,key=p.key,first=first*columns,count=count*columns,render=p.render},
                },
            },
        }
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
    function kit.BlurredArtwork(p) return ui.Image(props(p,{blur=12})) end
    return kit
end
