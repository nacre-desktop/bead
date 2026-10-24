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
        local opacity=p.on_activate~=nil and 1 or .4
        local out=props(p,{width=46,height=32,padding=0,fill="transparent",
            aria_label=checked and "On" or "Off"},{checked=true})
        out[1]=ui.Box{id="track",position="absolute",left=4,top=5,width=38,height=22,radius=11,
            fill=ui.alpha(checked and "primaryContainer" or "surfaceContainerHighest",opacity),
            border_width=1,border_color=ui.alpha("outline",.4*opacity),
            ui.Box{id="thumb",position="absolute",left=checked and 19 or 3,top=3,width=16,height=16,radius=8,
                fill=ui.alpha(checked and "onPrimaryContainer" or "onSurfaceVariant",opacity)}}
        return kit.Button(out)
    end
    -- QML NacreControlSlider: a 6 px rounded track, a filled part up to the value and an 8x16 handle, all inset 6 px.
    -- A horizontal two-colour fill (QML `Gradient` blending the accent into `secondary`): a flat `from` bar with
    -- round ends, and, while `tokens.gradients` is on (the `interfaceGradients` setting) and the bar is long
    -- enough, the interior blended in equal steps by laying `to` over it at rising alpha. Roles are palette
    -- names; `opacity` dims the whole fill. Off or short or dimmed, it is the flat `from` bar.
    function kit.Fill(p)
        local w,h,r=p.width,p.height,p.radius or p.height/2
        local opacity=p.opacity or 1
        local cap=2*r
        local out=props(p,{},{from=true,to=true,opacity=true})
        if tokens.gradients==false or opacity<1 or w<2*cap+4 then
            out.fill=ui.alpha(p.from,opacity)
            return ui.Box(out)
        end
        local inner=w-2*cap
        local steps=math.min(24,math.max(2,math.floor(inner/5)))
        local kids={ui.Box{id="from",position="absolute",left=0,top=0,width=cap+1,height=h,radius=r,fill=p.from},
            ui.Box{id="body",position="absolute",left=r,top=0,width=w-2*r,height=h,fill=p.from}}
        -- Each step lays `to` over everything from its left edge to the right end, so there is no seam to see; its alpha
        -- is what lifts the blend from the previous step's share to its own.
        local before=0
        for i=0,steps-1 do
            local t=(r+(i+.5)*(w-2*r)/steps)/w
            local alpha=1-(1-t)/(1-before)
            before=t
            local left=r+i*(w-2*r)/steps
            kids[#kids+1]=ui.Box{id="step-"..i,position="absolute",left=left,top=0,width=w-r-left,height=h,fill=ui.alpha(p.to,alpha)}
        end
        kids[#kids+1]=ui.Box{id="to",position="absolute",left=w-cap-1,top=0,width=cap+1,height=h,radius=r,fill=p.to}
        for i,node in ipairs(kids) do out[i]=node end
        return ui.Box(out)
    end
    -- Without `on_change` (or with `enabled=false`) the same drawing shows at 35 % and takes no input.
    function kit.Slider(p)
        local lo,hi=p.min or 0,p.max or 100
        local step=p.step or (hi-lo)/20
        assert(hi>lo and step>0 and math.ceil((hi-lo)/step)<=256,"invalid slider range/step")
        local first=p.change_arg and p.change_arg(lo) or lo
        local last=p.change_arg and p.change_arg(hi) or hi
        local key=(p.id or "").."|"..(p.on_change or "").."|"..typeof(first)..":"..tostring(first).."|"..typeof(last)..":"..tostring(last)
        local current=math.clamp(ui.preview_value(key,value(p.value,lo)),lo,hi)
        local width=p.width or 240
        local inset,height=6,30
        local avail=width-2*inset
        local fraction=(current-lo)/(hi-lo)
        local live=p.enabled~=false and p.on_change~=nil
        local opacity=live and 1 or .35
        local children={}
        if live then
            local count=math.ceil((hi-lo)/step)
            for i=0,count do
                local n=math.min(hi,lo+i*step)
                children[i+1]=ui.Button{drag=true,preview_key=key,preview_value=n,preview_inset=inset+4,id="value-"..i,grow=1,shrink=0,height=height,padding=0,radius=2,
                    fill=ui.alpha("surface",0),aria_label=(p.label or "Value").." "..n,
                    on_activate=ui.action(p.on_change,p.change_arg and p.change_arg(n) or n),
                    ui.Box{id="hit",width="100%",height="100%",fill=ui.alpha("surface",0)}}
            end
        end
        return ui.Box{id=p.id,width=width,height=height,position=p.position,left=p.left,top=p.top,
            ui.Box{id="track",position="absolute",left=inset,top=12,width=avail,height=6,radius=3,fill="surfaceContainerHighest"},
            kit.Fill{id="level",position="absolute",left=inset,top=12,width=fraction*avail,height=6,radius=3,
                from="primary",to="secondary",opacity=opacity},
            ui.Box{id="thumb",position="absolute",left=inset+fraction*(avail-8),top=7,width=8,height=16,radius=3,fill=ui.alpha("primary",opacity)},
            live and ui.Row(props(children,{id="hits",width="100%",height=height,gap=0})) or nil}
    end
    function kit.Tile(p)
        return ui.Row{id=p.id,gap=tokens.spacing.sm,fill="surfaceContainer",radius=tokens.radii.medium,
            kit.Button{id="toggle",text=p.text,grow=1,on_activate=p.on_toggle},
            kit.IconButton{id="detail",text="chevron_right",aria_label="Details",on_activate=p.on_detail}}
    end
    -- Control tiles: one rounded frame, four kinds. `on` fills a toggle, `available=false` dims and
    -- disarms it. Callers pass plain values and ready-made actions; sizes come from the scale.
    local glyph_size=tokens.icons.large*4/3
    local function ink(p)
        if p.available==false then return "onSurfaceVariant" end
        return p.on and "onPrimaryContainer" or "onSurface"
    end
    local function glyph(id,name,color,size)
        size=size or glyph_size
        return ui.Text{id=id,text=name,family="Material Symbols Rounded",font_size=size,width=size+2,height=24,shrink=0,color=color}
    end
    -- Geometry of the QML NacreControlTile: icon at 14, text from 44, 8 px right margin (24 more for a navigation
    -- chevron that sits 12 from the edge), 3 px between label and detail, a 1 px outline, radius 24 (20 stacked).
    local tile_glyph=(tokens.icons.large-1)*4/3
    local function tile(p,kind)
        local color,armed=ink(p),p.available~=false
        local pad=tokens.spacing.lg
        local main_w=p.width-(p.on_detail and 40 or 0)
        local caption=tokens.text.caption*4/3
        local small=(tokens.text.caption-1)*4/3
        local radius=p.stacked and tokens.radii.large or tokens.spacing.xxl
        local body
        if p.stacked then
            body=ui.Column{id="body",width=main_w,height=p.height,padding_top=9,gap=tokens.spacing.xs,align="center",
                ui.Text{id="icon",text=p.icon,family="Material Symbols Rounded",font_size=tile_glyph,width=main_w,height=24,text_align="center",color=color},
                ui.Text{id="label",text=p.label,width=main_w-2*tokens.spacing.xs,height=18,font_size=small,font_weight=500,text_align="center",color=color}}
        else
            local text_x=44
            local text_w=main_w-text_x-tokens.spacing.sm-(kind=="link" and tile_glyph+tokens.spacing.sm or 0)
            local label_size=tokens.text.caption*4/3
            local labels={ui.Text{id="label",text=p.label,width=text_w,height=18,font_size=label_size,font_weight=500,color=color}}
            if p.detail and p.detail~="" then
                -- network and Bluetooth names may take a second line, as in QML; the height follows the estimated wrap
                local lines=p.detail_lines and (#p.detail*small*.5>text_w and 2 or 1) or 1
                labels[2]=ui.Text{id="detail",text=p.detail,width=text_w,height=lines*16,font_size=small,
                    color=armed and p.on and "onPrimaryContainer" or "onSurfaceVariant"}
            end
            local children={glyph("icon",p.icon,color,tile_glyph),ui.Column(props(labels,{id="labels",width=text_w,gap=3}))}
            if kind=="link" then
                children[3]=ui.Text{id="chevron",position="absolute",left=main_w-12-tile_glyph,top=(p.height-24)/2,
                    text="chevron_right",family="Material Symbols Rounded",font_size=tile_glyph,width=tile_glyph+2,height=24,color=color}
            end
            body=ui.Row{id="body",width=main_w,height=p.height,padding_left=pad,gap=text_x-pad-tile_glyph-2,align="center",table.unpack(children)}
        end
        local kids={kit.Button{id="main",width=main_w,height=p.height,padding=0,radius=radius,fill="transparent",
            hover_fill=ui.alpha("onSurface",.08),aria_label=p.aria_label or p.label,on_activate=armed and p.on_activate or nil,body}}
        if p.on_detail then
            kids[2]=kit.Button{id="detail-open",width=40,height=p.height,padding=0,radius=radius,fill="transparent",
                hover_fill=ui.alpha("onSurface",.08),aria_label=p.detail_label or p.label,on_activate=p.on_detail,
                ui.Row{id="chevron-row",width=40,height=p.height,align="center",justify="center",glyph("chevron","chevron_right",color)}}
        end
        return ui.Row(props(kids,{id=p.id,width=p.width,height=p.height,radius=radius,align="center",
            position=p.position,left=p.left,top=p.top,border_width=1,border_color=ui.alpha("outline",armed and .12 or .06),
            fill=armed and (p.on and "primaryContainer" or "surfaceContainer") or ui.alpha("surfaceContainer",.4)}))
    end
    function kit.ToggleTile(p) return tile(p,"toggle") end
    function kit.LinkTile(p) return tile(props(p,{on=false,detail_lines=true}),"link") end
    function kit.ButtonTile(p) return tile(props(p,{on=false,stacked=true}),"button") end
    -- Level control (QML `NacreControlLevels`, 56 tall, radius 18): the caption (`caption`, else `label`) on top at 46, a
    -- 32 px icon button (mute) at 8, the stepped slider at 42, the value text 40 from the right and the details
    -- chevron 4 from it. The slider is drawn dimmed and takes no input when no `on_change` action is given.
    function kit.SliderTile(p)
        local color,armed=ink({available=p.available}),p.available~=false
        local W,H=p.width,p.height
        local detail=p.on_detail~=nil
        local right=detail and 40 or 8
        local control=kit.Slider{id="level",position="absolute",left=42,top=23,width=W-90-right,
            value=p.value,min=0,max=100,step=p.step or 5,label=p.label,enabled=armed,on_change=p.on_change,change_arg=p.change_arg}
        local kids={
            ui.Text{id="label",position="absolute",left=46,top=7,width=W-46-(detail and 86 or 54),height=16,font_size=(tokens.text.caption-1)*4/3,
                color="onSurfaceVariant",text=p.caption or p.label},
            kit.IconButton{id="toggle",position="absolute",left=8,top=17,width=32,height=32,padding=0,radius=tokens.radii.full,
                font_size=17*4/3,text=p.icon,fill="transparent",hover_fill=ui.alpha("onSurface",.08),color=armed and "primary" or "onSurfaceVariant",
                aria_label=p.toggle_label or p.label,on_activate=armed and p.on_toggle or nil},
            control,
            ui.Text{id="value",position="absolute",left=W-right-48,top=29,width=48,height=20,text_align="right",
                font_size=tokens.text.caption*4/3,color="onSurfaceVariant",text=p.detail or ""}}
        if detail then
            kids[#kids+1]=kit.IconButton{id="detail",position="absolute",left=W-36,top=22,width=32,height=32,padding=0,radius=tokens.radii.full,
                font_size=17*4/3,text="chevron_right",fill="transparent",hover_fill=ui.alpha("onSurface",.08),color=armed and "onSurface" or "onSurfaceVariant",
                aria_label=p.detail_label or p.label,on_activate=armed and p.on_detail or nil}
        end
        return ui.Box(props(kids,{id=p.id,width=W,height=H,radius=18,fill=ui.alpha("surfaceContainer",.65)}))
    end
    -- Packs tiles into rows by size: `sizes[size]={per_row,height}`; a tile is
    -- `{size,build=function(width,height) return node end}`. A change of size starts a new row. Returns the
    -- column and its height.
    function kit.TileGrid(p)
        local rows,current={},nil
        for _,t in ipairs(p.tiles) do
            local spec=p.sizes[t.size]
            if not current or current.size~=t.size or #current.items>=spec.per_row then
                current={size=t.size,items={}}
                table.insert(rows,current)
            end
            table.insert(current.items,t)
        end
        local out,total={},0
        for i,row in ipairs(rows) do
            local spec=p.sizes[row.size]
            -- a short last row shares the whole width (QML `Layout.fillWidth`)
            local cell=(p.width-p.gap*(#row.items-1))/#row.items
            local kids={}
            for j,t in ipairs(row.items) do kids[j]=t.build(cell,spec.height) end
            out[i]=ui.Row(props(kids,{id="row-"..i,width=p.width,height=spec.height,gap=p.gap}))
            total=total+spec.height+(i>1 and p.gap or 0)
        end
        return ui.Column(props(out,{id=p.id,width=p.width,height=total,gap=p.gap})),total
    end
    -- Rows the reader can reorder by dragging: one drag button per item under a single parent, so a
    -- press-and-move over another row fires that row's `on_touch` action (`bead.pressed` tells a new
    -- gesture from a continuing one). `items` are `{id,build=function(width,height) return node end}`;
    -- `remove` optionally builds a button per row, laid over its right end.
    function kit.ReorderList(p)
        local slots,overlay={},{}
        local pitch=p.item_height+p.gap
        for i,item in ipairs(p.items) do
            slots[i]=kit.Button{drag=true,id="slot-"..i,width=p.width,height=p.item_height,padding=0,radius=tokens.radii.medium,
                fill="surfaceContainer",hover_fill="surfaceContainerHigh",aria_label=item.label,on_activate=ui.action(p.on_touch,item.id),
                item.build(p.width,p.item_height)}
            if p.remove then
                overlay[i]=ui.Box{id="over-"..i,position="absolute",left=p.width-p.item_height,top=(i-1)*pitch,width=p.item_height,height=p.item_height,
                    p.remove(item,p.item_height)}
            end
        end
        local height=math.max(0,#p.items*pitch-p.gap)
        local children={ui.Column(props(slots,{id="slots",width=p.width,height=height,gap=p.gap}))}
        for _,node in ipairs(overlay) do table.insert(children,node) end
        return ui.Box(props(children,{id=p.id,width=p.width,height=height}))
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
        local out=props(p,{},{cells=true,extent=true,on_action=true,action_radius=true,action_release=true})
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
                    radius=p.action_radius or tokens.radii.small,activate_on_release=p.action_release,fill="surfaceContainerHigh",aria_label=item.t,on_activate=ui.action(p.on_action,item.a),
                    ui.Text{id="label",text=item.t,width=item.w,height=item.h-pad,font_size=item.s,text_align="center",color="onSurface"}}
            end}}
        return ui.Scroll(out)
    end
    -- Multi-line message box: Enter activates, Shift+Enter breaks the line.
    function kit.Composer(p)
        return kit.TextInput(props(p,{multiline=true,padding=tokens.spacing.md,radius=tokens.spacing.lg,fill="surfaceContainerHigh"}))
    end
    -- Secret input plus a show/hide button. `text` must be a mutable store named `*.secret`
    -- (kept out of traces and saved state); `revealed` is a bool, `on_toggle` flips it and
    -- `on_activate` runs on Enter. Masked fields stay out of accessibility, IME context and copy.
    function kit.PasswordField(p)
        local shown=value(p.revealed,false)
        local out=props(p,{gap=tokens.spacing.xs,align="center",height=40},
            {text=true,revealed=true,on_toggle=true,on_activate=true,placeholder=true,show_label=true,hide_label=true,aria_label=true})
        out[1]=kit.TextInput{id="field",text=p.text,placeholder=p.placeholder,masked=not shown,grow=1,
            aria_label=p.aria_label,on_activate=p.on_activate}
        out[2]=kit.IconButton{id="reveal",text=shown and "visibility_off" or "visibility",
            aria_label=shown and p.hide_label or p.show_label,on_activate=p.on_toggle}
        return ui.Row(out)
    end
    -- Text button for forms and prompts: centred label, `primary` fills with the primary role while
    -- armed; a nil `on_activate` dims it. `height` defaults to 36 and the button grows to share a row.
    function kit.ActionButton(p)
        local height=p.height or 36
        local armed=p.on_activate~=nil
        return kit.Button{id=p.id,height=height,grow=1,padding=0,padding_top=(height-24)/2,radius=tokens.radii.full,justify="center",align="center",
            fill=ui.alpha(p.primary and "primary" or "surfaceContainerHigh",armed and 1 or .4),aria_label=p.label,on_activate=p.on_activate,
            ui.Text{id=p.id.."-label",text=p.label,height=24,grow=1,font_size=tokens.text.body*4/3,text_align="center",
                color=p.primary and armed and "onPrimary" or (armed and "onSurface" or "onSurfaceVariant")}}
    end
    -- Row button that asks again in place: while `confirming` it shows `confirm_label` in the error
    -- colour with the same glyph and runs `on_confirm`; otherwise `label` runs `on_activate`.
    -- Rounded text/icon action; label_width is the measured advance supplied by the host.
    function kit.PillButton(p)
        local compact=p.compact==true
        local label=p.label or ""
        local size=(compact and tokens.text.caption or tokens.text.body)*4/3
        local icon=(compact and tokens.icons.small or tokens.icons.medium)*4/3
        local alpha=p.on_activate and 1 or .4
        local ink=ui.alpha(p.selected and "onPrimary" or "onSurface",alpha)
        -- Fractional-scale shaping can round the painted advance up by a pixel.
        local label_width=label~="" and math.ceil(p.label_width or #label*size*.55)+tokens.spacing.hairline or 0
        local children={}
        if p.glyph and p.glyph~="" then
            table.insert(children,ui.Text{id="icon",text=p.glyph,family="Material Symbols Rounded",font_size=icon,width=icon,height=24,color=ink})
        end
        if p.label and p.label~="" then
            table.insert(children,ui.Text{id="label",text=p.label,font_size=size,width=label_width,height=24,color=ink})
        end
        local padding=compact and tokens.spacing.xl or tokens.spacing.huge
        local text_width=label_width
        local has_icon=p.glyph and p.glyph~=""
        local extra=compact and (icon+tokens.spacing.xs+tokens.spacing.hairline) or (icon+tokens.spacing.xs+tokens.spacing.hairline/2)
        local width=padding+text_width+(has_icon and extra or 0)
        local base=p.selected and "primary" or "surfaceContainerHigh"
        local pressed=value(ui.bind("bead.pressed"),false)
        local out=props(p,{width=width,height=math.max(34,size+tokens.spacing.md),padding=0,radius=tokens.radii.full,activate_on_release=true,focus_ring=false,
            fill=ui.alpha(base,alpha),hover_fill=p.on_activate and ui.tint(base,"onSurface",pressed and .12 or .08) or ui.alpha(base,alpha),aria_label=p.label},
            {label=true,label_width=true,glyph=true,compact=true,selected=true})
        local gap=label~="" and has_icon and (compact and tokens.spacing.xs or tokens.spacing.sm) or 0
        local inset=math.max(0,(out.width-text_width-(has_icon and icon or 0)-gap)/2)
        out[1]=ui.Row{id="content",width=out.width,height=out.height,align="center",justify="center",padding_left=inset,
            padding_right=inset,gap=gap,table.unpack(children)}
        return kit.Button(out)
    end
    function kit.ConfirmButton(p)
        local confirming=value(p.confirming,false)
        local label=confirming and p.confirm_label or p.label
        local ink=confirming and "error" or (p.on_activate and "onSurface" or "onSurfaceVariant")
        local out=props(p,{},{label=true,confirm_label=true,glyph=true,confirming=true,on_confirm=true,on_activate=true})
        out.aria_label=label
        out.on_activate=confirming and p.on_confirm or p.on_activate
        out[1]=ui.Row{id=(p.id or "confirm").."-row",height=24,gap=tokens.spacing.md,align="center",
            p.glyph and ui.Text{id="glyph",text=p.glyph,width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=tokens.icons.large*4/3,color=ink} or nil,
            ui.Text{id="label",text=label,height=24,grow=1,font_size=tokens.text.body*4/3,color=ink}}
        return kit.Button(out)
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
        return ui.Box{id=p.id,width=p.width or 160,height=p.height or 8,radius=tokens.radii.full,fill=p.track or "surfaceContainerHigh",aria_label=p.label,
            ui.Box{id="level",width=(level*100).."%",height="100%",radius=tokens.radii.full,fill=p.ink or "primary"}}
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
                aria_label=p.label..": "..o.label,on_activate=maybe(ui.action(p.on_change,o.id),on and o.enabled~=false)}
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
    function kit.ListEditor(p)
        local on=p.enabled~=false
        local rows={}
        for i,item in ipairs(p.items) do
            local buttons={id="buttons",gap=sp.xs,align="center"}
            if p.reorder then
                local last=#p.items
                buttons[#buttons+1]=kit.IconButton{id="up",text="arrow_upward",aria_label=item.label.." "..(p.up_label or "up"),
                    on_activate=maybe(ui.action(p.on_remove,(p.remove_prefix or "")..(item.up or "")),on and i>1)}
                buttons[#buttons+1]=kit.IconButton{id="down",text="arrow_downward",aria_label=item.label.." "..(p.down_label or "down"),
                    on_activate=maybe(ui.action(p.on_remove,(p.remove_prefix or "")..(item.down or "")),on and i<last)}
            end
            buttons[#buttons+1]=kit.IconButton{id="remove",text="close",aria_label=p.remove_label.." "..item.label,on_activate=maybe(ui.action(p.on_remove,(p.remove_prefix or "")..item.id),on)}
            rows[i]=kit.FormRow{id="item-"..i,label=item.label,description="",height=40,enabled=on,ui.Row(buttons)}
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
    -- Label and value on one line (a read-only fact, a history entry): `description` goes under the label.
    function kit.InfoRow(p)
        return kit.FormRow{id=p.id,label=p.label,description=p.description,enabled=true,
            ui.Text{id="value",text=p.value or "",visible=(p.value or "")~="",width=math.min(360,8+9*(utf8.len(p.value or "") or 0)),shrink=0,
                height=24,font_size=tx.body*4/3,text_align="right",color="onSurfaceVariant"}}
    end
    -- Options too many for one line: label and description above, the options as wrapping chips below.
    -- `items`: `{id,label,selected,enabled}`; a click sends `on_change` with the item id.
    function kit.ChipsRow(p)
        local per_row=p.per_row or 4
        local lines=math.max(1,math.ceil(#p.items/per_row))
        -- A row without label (and description) is just its chips.
        local titled=(p.label or "")~=""
        return ui.Column{id=p.id,width="fill",shrink=0,gap=sp.xs,padding=sp.sm,height=sp.sm*2+(titled and 36 or 0)+lines*38,
            ui.Text{id="label",text=p.label,visible=titled,height=20,font_size=tx.body*4/3,color=p.enabled~=false and "onSurface" or "onSurfaceVariant"},
            ui.Text{id="description",text=p.description or "",visible=(p.description or "")~="",height=16,font_size=tx.caption*4/3,color="onSurfaceVariant"},
            kit.ChipRow{id="chips",items=p.items,per_row=per_row,on_change=p.on_change}}
    end
    -- A device or entity line: icon, title and detail on the left, buttons on the right, and an optional
    -- slider line below (`slider`: `{value,min,max,step,label,prefix,text,enabled}`; the new value goes to
    -- `on_action` as `prefix..value`). `buttons`: `{id,label,icon,selected,enabled}`; `id` is the argument.
    function kit.DeviceRow(p)
        local on=p.enabled~=false
        local buttons={id="buttons",gap=sp.xs,align="center"}
        for i,b in ipairs(p.buttons or {}) do
            local armed=on and b.enabled~=false
            local action=maybe(ui.action(p.on_action,b.id),armed)
            if b.icon then
                buttons[#buttons+1]=kit.IconButton{id="button-"..i,text=b.icon,selected=b.selected,aria_label=b.label,on_activate=action,
                    fill=b.selected and "primaryContainer" or "surfaceContainer",color=b.selected and "onPrimaryContainer" or "onSurface"}
            else
                buttons[#buttons+1]=kit.Button{id="button-"..i,text=b.label,height=32,padding=sp.xs,padding_left=sp.sm,padding_right=sp.sm,shrink=0,
                    width=sp.xxl+(utf8.len(b.label) or 4)*8,selected=b.selected,
                    fill=b.selected and "primaryContainer" or "surfaceContainer",color=b.selected and "onPrimaryContainer" or "onSurface",
                    aria_label=p.title..": "..b.label,on_activate=action}
            end
        end
        local head={id=p.slider and "head" or p.id,width="fill",height=56,shrink=0,gap=sp.md,align="center",padding_left=sp.sm,padding_right=sp.sm,
            radius=tokens.radii.small}
        if p.icon then
            head[#head+1]=ui.Text{id="icon",text=p.icon,width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=tokens.icons.large*4/3,color="onSurfaceVariant"}
        end
        head[#head+1]=ui.Column{id="labels",grow=1,gap=sp.hairline,justify="center",
            ui.Text{id="label",text=p.title,font_size=tx.body*4/3,height=20,color=on and "onSurface" or "onSurfaceVariant"},
            ui.Text{id="description",text=p.detail or "",visible=(p.detail or "")~="",font_size=tx.caption*4/3,height=16,color="onSurfaceVariant"}}
        head[#head+1]=ui.Row(buttons)
        head=ui.Row(head)
        if not p.slider then return head end
        local s=p.slider
        return ui.Column{id=p.id,width="fill",shrink=0,gap=0,head,
            ui.Row{id="level",width="fill",height=36,shrink=0,gap=sp.md,align="center",padding_left=sp.sm,padding_right=sp.sm,
                s.enabled and kit.Slider{id="slider",value=s.value,min=s.min,max=s.max,step=s.step,width=320,label=s.label,
                    on_change=p.on_action,change_arg=function(n) return s.prefix..n end} or kit.LevelMeter{id="slider",value=s.value/(s.max-s.min),width=320},
                ui.Text{id="percent",text=s.text,width=48,height=24,font_size=tx.caption*4/3,text_align="right",color="onSurface"}}}
    end
    -- A searchable pick list: a filter field (`field` store), the matching entries and a count of the rest.
    -- `items`: `{id,label,selected}`; a click sends `on_pick` with the item id.
    function kit.PickerList(p)
        local on=p.enabled~=false
        local rows={id="entries",width="fill",gap=sp.hairline,shrink=0}
        for i,item in ipairs(p.items) do
            rows[#rows+1]=kit.Button{id="entry-"..i,text=item.label,height=36,width="fill",text_align="left",padding_left=sp.md,
                selected=item.selected,fill=item.selected and "primaryContainer" or "surfaceContainer",
                color=item.selected and "onPrimaryContainer" or "onSurface",aria_label=item.label,
                on_activate=maybe(ui.action(p.on_pick,item.id),on)}
        end
        if #p.items==0 then
            rows[#rows+1]=ui.Text{id="empty",text=p.empty,height=20,font_size=tx.caption*4/3,color="onSurfaceVariant"}
        end
        if (p.more or "")~="" then
            rows[#rows+1]=ui.Text{id="more",text=p.more,height=16,font_size=tx.caption*4/3,color="onSurfaceVariant"}
        end
        return ui.Column{id=p.id,width="fill",shrink=0,gap=sp.xs,padding=sp.sm,
            ui.Text{id="label",text=p.label,height=20,font_size=tx.body*4/3,color=on and "onSurface" or "onSurfaceVariant"},
            kit.TextInput{id="filter",text=p.field,placeholder=p.placeholder,width="fill",aria_label=p.placeholder},
            ui.Column(rows)}
    end
    -- Ask for a secret in a form: label, the masked field (`field` is a `*.secret` store) and two buttons.
    function kit.SecretRow(p)
        local on=p.enabled~=false
        return kit.FormRow{id=p.id,label=p.label,description="",enabled=on,height=56,
            kit.PasswordField{id="password",text=p.field,revealed=p.revealed,on_toggle=p.on_toggle,on_activate=maybe(p.on_join,on),width=240,
                placeholder=p.label,show_label=p.show_label,hide_label=p.hide_label,aria_label=p.label},
            kit.Button{id="join",text=p.join_label,width=88,height=32,padding=sp.xs,fill="primaryContainer",color="onPrimaryContainer",on_activate=maybe(p.on_join,on)},
            kit.Button{id="cancel",text=p.cancel_label,width=88,height=32,padding=sp.xs,on_activate=p.on_cancel}}
    end
    -- A choice that needs an explicit apply: shows what is pending and offers both ways out. The buttons keep
    -- their width (`shrink=0`); the sentence takes what is left and clips.
    function kit.ApplyBar(p)
        local function button(id,label,action,primary)
            return kit.Button{id=id,text=label,width=96,height=32,shrink=0,padding=sp.xs,radius=tokens.radii.full,
                fill=primary and "primary" or "surfaceContainerHigh",color=primary and "onPrimary" or "onSurface",
                aria_label=label,on_activate=action}
        end
        return ui.Row{id=p.id,width="fill",height=40,shrink=0,gap=sp.sm,align="center",padding_left=sp.sm,padding_right=sp.sm,
            radius=tokens.radii.small,fill="secondaryContainer",
            ui.Text{id="pending",text=p.text,grow=1,shrink=1,height=20,font_size=tx.caption*4/3,color="onSecondaryContainer"},
            button("apply",p.apply_label,p.on_apply,true),button("discard",p.discard_label,p.on_discard,false)}
    end
    -- A drop-down (QML `NacreSelect`): a field (`text` is the chosen option's label) with the list under it
    -- while `open`. The kit has no overlay layer, so the list is part of the layout and the rows below move
    -- down. `options`: `{id,label,selected,enabled}`; a click sends `on_change` with the option id, `on_toggle`
    -- opens and closes. The caller keeps `open` (and the list's `scroll` store) and closes it on a choice and
    -- on Escape. `field` (a store) adds a filter line above the list: the caller passes only the matching
    -- options, `empty` is shown for none and `more` for a cut-off tail.
    local SELECT_ROW=sp.huge+sp.sm
    local function select_parts(p)
        local on=p.enabled~=false
        local open=value(p.open,false) and on
        local width=value(p.width,sp.huge*8)
        local field=kit.Button{id="field",width=width,height=SELECT_ROW,padding=0,shrink=0,radius=tokens.radii.small,
            fill="surfaceContainer",hover_fill=on and "surfaceContainerHigh" or "surfaceContainer",border_width=1,
            border_color=open and "primary" or "outline",aria_label=p.aria_label or p.label,on_activate=maybe(p.on_toggle,on),
            ui.Row{id="field-row",width=width,height=SELECT_ROW,align="center",padding_left=sp.md,padding_right=sp.sm,
                ui.Text{id="value",text=p.text or "",grow=1,shrink=1,height=24,font_size=tx.body*4/3,color=on and "onSurface" or "onSurfaceVariant"},
                ui.Text{id="indicator",text="expand_more",family="Material Symbols Rounded",font_size=tokens.icons.medium*4/3,
                    width=tokens.icons.medium*4/3+2,height=24,shrink=0,color=on and "onSurface" or "onSurfaceVariant"}}}
        if not open then return field,nil,width end
        local searchable=p.field~=nil
        local pop_w=searchable and math.max(width,sp.huge*11) or width
        local inner=pop_w-2*sp.sm-2
        local count=#p.options
        local list_h=math.min(math.max(count,1)*SELECT_ROW,sp.huge*9)
        local rows={}
        for i,o in ipairs(p.options) do
            local picked=o.selected==true
            rows[i]=kit.Button{id="option-"..i,width=inner,height=SELECT_ROW,padding=0,shrink=0,radius=tokens.radii.small,
                selected=picked,fill=picked and "primary" or "transparent",hover_fill=picked and "primary" or ui.alpha("primary",.15),
                aria_label=o.label,on_activate=maybe(ui.action(p.on_change,o.id),on and o.enabled~=false),
                ui.Row{id="option-row",width=inner,height=SELECT_ROW,align="center",padding_left=sp.md,padding_right=sp.md,
                    ui.Text{id="label",text=o.label,grow=1,shrink=1,height=24,font_size=tx.body*4/3,color=picked and "onPrimary" or "onSurface"}}}
        end
        if count==0 and p.empty then
            rows[1]=ui.Text{id="empty",text=p.empty,width=inner,height=SELECT_ROW,padding_left=sp.md,font_size=tx.caption*4/3,color="onSurfaceVariant"}
        end
        local list
        if count*SELECT_ROW>list_h and p.scroll~=nil then
            rows.id="entries";rows.width=inner
            list=ui.Scroll{id="list",scroll=p.scroll,width=inner,height=list_h,shrink=0,ui.Column(rows)}
        else
            rows.id="list";rows.width=inner;rows.height=list_h;rows.shrink=0
            list=ui.Column(rows)
        end
        local kids={}
        local height=2*sp.sm+2+list_h
        if searchable then
            kids[1]=kit.TextInput{id="filter",text=p.field,placeholder=p.placeholder,width=inner,height=sp.huge,shrink=0,padding=sp.xs,
                padding_left=sp.sm,radius=tokens.radii.medium,fill="surfaceContainer",aria_label=p.placeholder}
            height=height+sp.huge+sp.sm
        end
        kids[#kids+1]=list
        if (p.more or "")~="" then
            kids[#kids+1]=ui.Text{id="more",text=p.more,width=inner,height=16,shrink=0,font_size=tx.caption*4/3,color="onSurfaceVariant"}
            height=height+16+sp.sm
        end
        kids.id="popup";kids.width=pop_w;kids.height=height;kids.shrink=0;kids.padding=sp.sm;kids.gap=sp.sm
        kids.radius=tokens.radii.medium;kids.fill="surfaceContainer";kids.border_width=1;kids.border_color="outline"
        return field,ui.Column(kids),pop_w,height
    end
    -- The field alone with its list under it, both `width` wide.
    function kit.Select(p)
        local field,popup=select_parts(p)
        return ui.Column{id=p.id,width=p.width or sp.huge*8,shrink=0,gap=sp.xs,field,popup}
    end
    -- A form row (label and description on the left) whose control is a select; the list opens under the row,
    -- right-aligned with the field.
    function kit.SelectRow(p)
        local field,popup,pop_w,height=select_parts(p)
        local row=kit.FormRow{id="row",label=p.label,description=p.description,enabled=p.enabled,field}
        if not popup then return ui.Column{id=p.id,width="fill",shrink=0,row} end
        return ui.Column{id=p.id,width="fill",shrink=0,gap=sp.xs,row,
            ui.Row{id="popup-row",width="fill",height=height,shrink=0,justify="end",padding_right=sp.sm,popup}}
    end
    function kit.BlurredArtwork(p) return ui.Image(props(p,{blur=12})) end
    -- Wrapped text. Callers may supply their font's average advance in em (default 0.56).
    -- `max_lines` ends with an ellipsis.
    function kit.Paragraph(p)
        local size=p.font_size or tokens.text.body*4/3
        local advance=p.advance or .56
        assert(type(advance)=="number" and advance>0,"advance must be positive")
        local per=math.max(1,math.floor(p.width/(size*advance)))
        local lines,line={},""
        for word in string.gmatch(p.text or "","%S+") do
            local joined=line=="" and word or line.." "..word
            if line=="" or (utf8.len(joined) or #joined)<=per then line=joined else table.insert(lines,line);line=word end
        end
        if line~="" then table.insert(lines,line) end
        if p.max_lines and #lines>p.max_lines then
            while #lines>p.max_lines do table.remove(lines) end
            local last=tostring(lines[#lines])
            local n=utf8.len(last) or #last
            lines[#lines]=(n>1 and string.sub(last,1,(utf8.offset(last,n) or 1)-1) or "").."…"
        end
        local lh=math.ceil(size*1.35)
        local kids={}
        for i,l in ipairs(lines) do
            kids[i]=ui.Text{id="line-"..i,text=l,width=p.width,height=lh,font_size=size,text_align=p.text_align,color=p.color or "onSurface"}
        end
        return ui.Column(props(kids,{id=p.id,width=p.width,height=math.max(lh,#lines*lh),shrink=0,gap=0,position=p.position,left=p.left,top=p.top}))
    end
    -- Colour pieces (B0060). Colours are RGBA arrays from the caller (any fill accepts them); labels,
    -- actions and sizes are plain arguments, so nothing here knows a palette role set or a product.
    local mono="JetBrainsMono Nerd Font"
    -- A swatch over a small caption, one button; `selected` draws the ring.
    function kit.ColorSample(p)
        return kit.Button{id=p.id,width=p.width,grow=p.width and 0 or 1,shrink=p.width and 0 or 1,height=p.height or 60,padding=0,radius=tokens.radii.small,fill="transparent",
            hover_fill=ui.alpha("onSurface",.08),aria_label=p.aria_label or p.caption,on_activate=p.on_activate,
            ui.Column{id="sample",width="fill",height="100%",gap=sp.xs,
                ui.Box{id="swatch",width="fill",height=34,radius=tokens.radii.small,fill=p.color,
                    border_width=p.selected and 2 or 0,border_color="onSurface"},
                ui.Text{id="caption",text=p.caption or "",width="fill",height=18,font_size=9*4/3,text_align="center",color="onSurfaceVariant"}}}
    end
    -- `items`: `{id,color,caption,aria_label,action}`; one row of equal samples.
    function kit.ColorSampleRow(p)
        local kids={}
        for i,item in ipairs(p.items) do
            kids[i]=kit.ColorSample{id=item.id,color=item.color,caption=item.caption,aria_label=item.aria_label,
                selected=item.selected,on_activate=item.action,height=p.height,width=p.sample_width}
        end
        return ui.Row(props(kids,{id=p.id,width="fill",height=p.height or 60,shrink=0,gap=7}))
    end
    -- A named colour slot: swatch, label and value; the main area is one button (`on_arm`), the
    -- lock button pins or releases the slot (`on_lock`). `armed` rings the slot, waiting for a sample.
    function kit.ColorTarget(p)
        return ui.Row{id=p.id,width="fill",height=54,shrink=0,gap=0,align="center",radius=12,fill="surfaceContainer",
            border_width=p.armed and 2 or 1,border_color=p.armed and "primary" or "outline",
            kit.Button{id="main",grow=1,height=52,padding=0,padding_left=12,radius=12,fill="transparent",hover_fill=ui.alpha("onSurface",.06),
                aria_label=p.aria_label or p.label,on_activate=p.on_arm,
                ui.Row{id="main-row",grow=1,height=52,gap=10,align="center",
                    ui.Box{id="swatch",width=30,height=30,radius=tokens.radii.small,fill=p.color,shrink=0},
                    ui.Text{id="label",text=p.label,width=64,height=20,shrink=0,font_size=tokens.text.caption*4/3,color="onSurface"},
                    ui.Text{id="value",text=p.value or "",grow=1,height=20,family=mono,font_size=9*4/3,color="onSurfaceVariant"}}},
            kit.IconButton{id="lock",text=p.locked and "lock" or "lock_open",aria_label=p.lock_label,width=34,height=34,padding=0,
                fill="transparent",on_activate=p.on_lock}}
    end
    -- Tabs as text with an underline on the chosen one. `items`: `{id,label,selected}`.
    function kit.TabStrip(p)
        local kids={}
        for i,item in ipairs(p.items) do
            kids[i]=kit.Button{id=item.id,height=42,padding=0,padding_left=sp.sm,padding_right=sp.sm,radius=tokens.radii.full,
                fill="transparent",hover_fill=ui.alpha("onSurface",.06),aria_label=item.label,on_activate=ui.action(p.on_change,p.change_arg and p.change_arg(item.id) or item.id),
                ui.Column{id=item.id.."-col",height=42,justify="end",align="center",
                    ui.Text{id="label",text=item.label,height=38,width=item.label and (#item.label*8+4) or 40,font_size=tokens.text.caption*4/3,text_align="center",
                        color=item.selected and "onSurface" or "onSurfaceVariant"},
                    ui.Box{id="line",width="100%",height=2,radius=1,fill=item.selected and "primary" or "transparent"}}}
        end
        return ui.Row(props(kids,{id=p.id,height=42,gap=sp.xl,align="center"}))
    end
    -- Compact toggle chips, `per_row` to a line (the kit has no wrapping row). `items`: `{id,label,selected}`.
    function kit.ChipRow(p)
        local lines,line={},nil
        for i,item in ipairs(p.items) do
            if not line or #line>=(p.per_row or 4) then line={};table.insert(lines,line) end
            table.insert(line,kit.Button{id=item.id,height=32,padding=sp.xs,radius=p.radius,padding_left=sp.md,padding_right=sp.md,shrink=0,
                width=(item.width or 2*sp.md+8+(utf8.len(item.label) or 4)*9),text=item.label,selected=item.selected,
                fill=item.selected and "primaryContainer" or "surfaceContainer",color=item.selected and "onPrimaryContainer" or "onSurface",
                aria_label=item.label,on_activate=maybe(ui.action(p.on_change,p.change_arg and p.change_arg(item.id) or item.id),item.enabled~=false)})
        end
        local rows={}
        for i,l in ipairs(lines) do rows[i]=ui.Row(props(l,{id="line-"..i,gap=6})) end
        return ui.Column(props(rows,{id=p.id,gap=6,shrink=0}))
    end
    -- An image with an invisible `cols` x `rows` hit grid; a hit sends the cell centre as fractions of
    -- the box: `on_pick` is an action name and `pick_arg(x,y)` builds its argument. The image slot
    -- (`icon`) is filled by the host; the host maps fractions to the painted part of the image.
    function kit.ImagePick(p)
        local cols,rows=p.cols or 16,p.rows or 10
        local lines={}
        for r=0,rows-1 do
            local cells={}
            for c=0,cols-1 do
                cells[c+1]=ui.Button{drag=false,id="cell-"..c,grow=1,height="100%",padding=0,radius=0,fill=ui.alpha("surface",0),
                    aria_label=p.aria_label,on_activate=ui.action(p.on_pick,p.pick_arg((c+.5)/cols,(r+.5)/rows)),
                    ui.Box{id="hit",width="100%",height="100%",fill=ui.alpha("surface",0)}}
            end
            lines[r+1]=ui.Row(props(cells,{id="row-"..r,width="100%",grow=1,gap=0}))
        end
        return ui.Box{id=p.id,width=p.width,height=p.height,radius=p.radius or 0,
            ui.Image{id="image",position="absolute",left=0,top=0,width=p.width,height=p.height,icon=p.icon,aria_label=p.aria_label},
            ui.Column(props(lines,{id="hits",position="absolute",left=0,top=0,width=p.width,height=p.height,gap=0}))}
    end
    -- A miniature desktop drawn from a colour map (role -> RGBA): bar with logo mark and workspace
    -- chips, a window, a terminal and a notification. `p.labels` carries the sample texts and
    -- `p.image` the wallpaper slot (cropped by the host). Geometry follows the card size.
    function kit.PreviewCard(p)
        local w,h=p.width,p.height
        local colors=p.colors or {}
        local fallback=colors.onSurface or {232,232,232,255}
        local function c(role) return colors[role] or fallback end
        local L=p.labels or {}
        local condensed=p.compact or h<260
        local win_h=p.compact and math.min(100,h-50) or condensed and 80 or math.min(160,h-160)
        local win_y=condensed and 40 or 50
        local small=9*4/3
        local kids={}
        local function add(node) table.insert(kids,node) end
        if p.image and p.image~="" then
            add(ui.Image{id="wallpaper",position="absolute",left=5,top=condensed and 30 or 35,width=w-10,height=h-(condensed and 35 or 40),icon=p.image,aria_label=L.wallpaper})
        end
        for i,role in ipairs{"primary","secondary","tertiary"} do
            add(ui.Box{id="mark-"..i,position="absolute",left=10+(i-1)*5,top=10+(i-1)*3,width=10,height=10,radius=3,fill=c(role)})
        end
        for i,role in ipairs{"primary","secondary","tertiary"} do
            local filled=i==1 or p.workspace_colors
            local on={"onPrimary","onSecondary","onTertiary"}
            add(ui.Box{id="ws-"..i,position="absolute",left=40+(i-1)*29,top=8,width=24,height=20,radius=5,fill=filled and c(role) or "transparent",
                ui.Text{id="n",position="absolute",left=0,top=2,width=24,height=16,text=tostring(i),font_size=8*4/3,text_align="center",color=filled and c(on[i]) or c("onSurface")}})
        end
        add(ui.Text{id="brand",position="absolute",left=w-73,top=10,width=60,height=16,text=L.brand or "",font_size=small,color=c("onSurface")})
        local win={ui.Box{id="head",position="absolute",left=0,top=0,width=w-28,height=27,radius=9,fill=c("surfaceContainerHigh")},
            ui.Text{id="title",position="absolute",left=12,top=6,width=120,height=16,text=L.files or "",font_size=small,color=c("onSurface")}}
        for i,line in ipairs(L.lines or {}) do
            if i<=(condensed and 2 or 3) then
                table.insert(win,ui.Text{id="line-"..i,position="absolute",left=12,top=40+(i-1)*15.6,width=120,height=16,text=line,font_size=small,color=c("onSurface")})
            end
        end
        table.insert(win,ui.Box{id="open",position="absolute",left=w-28-105,top=44,width=90,height=29,radius=6,fill=c("primary"),
            ui.Text{id="label",position="absolute",left=0,top=6,width=90,height=16,text=L.open or "",font_size=small,text_align="center",color=c("onPrimary")}})
        table.insert(win,ui.Box{id="rule",position="absolute",left=w-28-105,top=condensed and 76 or 84,width=90,height=3,fill=c("secondary")})
        add(ui.Box(props(win,{id="window",position="absolute",left=14,top=win_y,width=w-28,height=win_h,radius=9,fill=c("surfaceContainer")})))
        if not p.compact then
            local th=condensed and 68 or 82
            add(ui.Box{id="terminal",position="absolute",left=14,top=h-th-10,width=(w-34)*.52,height=th,radius=7,fill=c("surfaceContainerLowest"),
                ui.Text{id="t1",position="absolute",left=9,top=condensed and 7 or 9,width=120,height=14,text=L.build or "",family=mono,font_size=8*4/3,color=c("onSurface")},
                ui.Text{id="t2",position="absolute",left=9,top=condensed and 27 or 31,width=120,height=14,text=L.ready or "",font_size=8*4/3,color=c("green")},
                ui.Text{id="t3",position="absolute",left=9,top=condensed and 45 or 53,width=120,height=14,text=L.warning or "",font_size=8*4/3,color=c("yellow")}})
            local nw=w*.43-14
            add(ui.Box{id="notice",position="absolute",left=w*.57,top=h-67-10,width=nw,height=67,radius=8,fill=c("primaryContainer"),
                ui.Text{id="n1",position="absolute",left=9,top=9,width=nw-18,height=14,text=L.message or "",font_size=8*4/3,color=c("onPrimaryContainer")},
                ui.Text{id="n2",position="absolute",left=9,top=24,width=nw-18,height=14,text=L.message_detail or "",font_size=8*4/3,color=c("onPrimaryContainer")}})
        end
        return ui.Box(props(kids,{id=p.id,width=w,height=h,shrink=0,radius=14,fill=c("frame"),border_width=1,border_color=c("outline"),aria_label=p.aria_label}))
    end
    return kit
end
