-- SPDX-License-Identifier: GPL-3.0-only
-- Colors app (B0060): wallpaper colour explorer. Header (title, tabs, preview mode), a wallpaper
-- rail and a scrolling canvas for Studio, Compare and Accessibility, and an Export/Apply footer.
-- All data is plain projection from `colors.rs`; sizes come from the window width and the token scale.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local sp,tx,rd=T.spacing,T.text,T.radii
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local function px(pt) return pt*4/3 end
local function act(arg) return ui.action("colors.do",arg) end
local W=get("colors.win_w")
local page=get("colors.page")
local MARGIN=sp.xxl-8
local RAIL=W<980 and 180 or 210
local CANVAS=W-2*MARGIN-RAIL-MARGIN
local split=CANVAS>=720
local COL1=split and (CANVAS-20)*.58 or CANVAS
local COL2=split and (CANVAS-20)*.42 or CANVAS
local function label(id,text,size,color,height)
    return ui.Text{id=id,text=text,width="fill",height=height or math.ceil(px(size)*1.35),shrink=0,font_size=px(size),color=color or "onSurface"}
end
-- Compact text button with an optional Material glyph; `action=nil` dims it.
local function button(id,text,glyph,action,selected)
    local armed=action~=nil
    local label=tostring(text)
    local chars=utf8.len(label) or 4
    local ink=selected and armed and "onPrimary" or (armed and "onSurface" or "onSurfaceVariant")
    local row={}
    if glyph then table.insert(row,ui.Text{id="glyph",text=glyph,family="Material Symbols Rounded",width=24,height=24,shrink=0,font_size=px(T.icons.large),color=ink}) end
    if label~="" then table.insert(row,ui.Text{id="label",text=label,height=24,shrink=0,width=chars*8+4,font_size=px(tx.body),color=ink}) end
    row.id="row";row.height=32;row.gap=sp.xs;row.align="center";row.justify="center"
    local span=2*sp.md+(glyph and 24+sp.xs or 0)+(label~="" and chars*8+4 or 0)
    return kit.Button{id=id,width=span,height=32,shrink=0,padding=0,padding_left=sp.md,padding_right=sp.md,radius=rd.small,
        fill=selected and (armed and "primary" or "surfaceContainerHigh") or "surfaceContainer",aria_label=label~="" and label or glyph,on_activate=action,ui.Row(row)}
end
local function header(v)
    local tabs={}
    for i,tab in ipairs{{"studio",ui.tr("bead.colors.studio")},{"compare",ui.tr("bead.colors.compare")},{"accessibility",ui.tr("bead.colors.accessibility")}} do
        tabs[i]={id=tab[1],label=tab[2],selected=page==tab[1]}
    end
    local modes={}
    for i,m in ipairs{{"dark","dark_mode","bead.colors.dark-preview"},{"light","light_mode","bead.colors.light-preview"}} do
        modes[i]=kit.IconButton{id="mode-"..m[1],width=36,height=32,padding=0,radius=rd.full,text=m[2],
            selected=v.mode==m[1],fill=v.mode==m[1] and "primary" or "surfaceContainer",color=v.mode==m[1] and "onPrimary" or "onSurface",
            aria_label=ui.tr(m[3]),on_activate=act("mode:"..m[1])}
    end
    modes.id="modes";modes.gap=sp.xs
    return ui.Row{id="header",width="fill",height=64,shrink=0,align="center",gap=14,padding_left=sp.xl,padding_right=sp.xl,
        ui.Image{id="logo",path="colors/assets/nacre-colors.themed.svg",width=T.icons.hero,height=T.icons.hero,shrink=0,aria_label=ui.tr("bead.colors.title")},
        ui.Text{id="title",text=ui.tr("bead.colors.title"),height=28,width=200,shrink=0,font_size=px(tx.title)},
        ui.Box{id="space",grow=1,height=1},
        kit.TabStrip{id="tabs",items=tabs,on_change="colors.do",change_arg=function(id) return "page:"..id end},
        ui.Row(modes)}
end
local function entry(item)
    local ink="onSurface"
    return kit.Button{id="entry",width="fill",height=88,shrink=0,padding=0,radius=10,
        fill=item.selected and ui.alpha("primary",.14) or "transparent",hover_fill=ui.alpha("onSurface",.06),
        border_width=item.selected and 1 or 0,border_color=ui.alpha("primary",.65),aria_label=item.name,on_activate=act(item.action),
        ui.Box{id="body",width="100%",height="100%",
            item.thumb~="" and ui.Image{id="thumb",position="absolute",left=8,top=8,width=60,height=42,icon=item.thumb,aria_label=item.name}
                or ui.Box{id="thumb",position="absolute",left=8,top=8,width=60,height=42,radius=4,fill="surfaceContainerHigh"},
            kit.Paragraph{id="name",position="absolute",left=77,top=10,text=item.name,width=RAIL-16-77-10,font_size=px(10),max_lines=3,color=ink},
            ui.Text{id="kind",position="absolute",left=8,top=62,width=RAIL-32,height=18,text=item.kind,font_size=px(9),color="onSurfaceVariant"}}}
end
local function rail()
    return ui.Column{id="rail",width=RAIL,height="100%",shrink=0,radius=18,fill="surfaceContainer",padding=sp.md,gap=sp.sm,
        kit.TextInput{id="search",text=ui.bind("colors.query"),placeholder=ui.tr("bead.colors.find-wallpaper"),width="fill",
            aria_label=ui.tr("bead.colors.find-wallpaper")},
        ui.Text{id="empty",visible=ui.bind("colors.empty"),text=ui.tr("bead.colors.no-matches"),width="fill",height=40,font_size=px(10),color="onSurfaceVariant"},
        ui.Scroll{id="entries",scroll=ui.bind("colors.rail.scroll"),grow=1,
            ui.Column{id="list",width="fill",gap=6,ui.Repeat{id="items",model=ui.bind("colors.entries"),key="id",render=entry}}}}
end
local function studio(v)
    local pw=math.min(COL1,821)
    local ph=pw*.56
    local photo
    if v.has_photo then
        photo=kit.ImagePick{id="photo",width=pw,height=ph,icon="colors-photo",cols=16,rows=10,on_pick="colors.do",
            pick_arg=function(x,y) return string.format("pick:%.4f,%.4f",x,y) end,aria_label=ui.tr("bead.colors.pick-hint")}
    else
        photo=ui.Box{id="photo",width=pw,height=ph,radius=rd.small,fill="surfaceContainerLow",shrink=0}
    end
    local chips={}
    for i,s in ipairs(v.styles) do chips[i]={id=s.id,label=s.label,selected=s.selected,enabled=v.available} end
    local samples={}
    for i,sample in ipairs(v.samples) do
        samples[i]={id=sample.id,color=sample.color,caption=sample.caption,aria_label=sample.aria_label,
            action=(sample.action and sample.action~="") and act(sample.action) or nil}
    end
    local left={id="left",width=COL1,gap=12,shrink=0,
        photo,
        label("families",ui.tr("bead.colors.families"),11),
        kit.ColorSampleRow{id="samples",items=samples,sample_width=84}}
    if v.other~="" then table.insert(left,ui.Text{id="other",text=v.other,width="fill",height=16,font_size=px(9),color="onSurfaceVariant"}) end
    table.insert(left,kit.Paragraph{id="grouped",text=ui.tr("bead.colors.grouped-note"),width=COL1,font_size=px(9),color="onSurfaceVariant"})
    table.insert(left,label("style-title",ui.tr("bead.colors.color-style"),12))
    table.insert(left,kit.ChipRow{id="styles",items=chips,per_row=CANVAS>=620 and 7 or 4,radius=rd.full,on_change="colors.do",change_arg=function(id) return "style:"..id end})
    table.insert(left,kit.Paragraph{id="style-text",text=v.style_text,width=COL1,font_size=px(10),color="onSurfaceVariant"})
    local targets={}
    for i,t in ipairs(v.targets) do
        targets[i]=kit.ColorTarget{id="target-"..t.id,color=t.color,label=t.label,value=t.value,locked=t.locked,armed=t.armed,
            lock_label=t.lock_label,on_arm=act(t.arm),on_lock=act(t.lock)}
    end
    targets.id="targets";targets.width="fill";targets.gap=6;targets.shrink=0
    local right={id="right",width=COL2,gap=12,shrink=0,
        label("preview-title",ui.tr("bead.colors.desktop-preview"),12),
        kit.PreviewCard{id="preview",width=COL2,height=math.min(280,COL2*.75),colors=v.colors,image=v.has_photo and "colors-card" or "",
            workspace_colors=v.workspace_colors,aria_label=ui.tr("bead.colors.preview-aria"),labels={wallpaper="",brand=ui.tr("bead.colors.card-brand"),files=ui.tr("bead.colors.card-files"),
                lines={ui.tr("bead.colors.card-documents"),ui.tr("bead.colors.card-pictures"),ui.tr("bead.colors.card-projects")},open=ui.tr("bead.colors.card-open"),
                build=ui.tr("bead.colors.card-build"),ready=ui.tr("bead.colors.card-ready"),warning=ui.tr("bead.colors.card-warning"),
                message=ui.tr("bead.colors.card-message"),message_detail=ui.tr("bead.colors.card-message-detail")}},
        ui.Text{id="readable",text=v.readable,width="fill",height=16,font_size=px(10),color="onSurfaceVariant"},
        ui.Column(targets),
        button("fine",v.fine and ui.tr("bead.colors.hide-fine-tuning") or ui.tr("bead.colors.fine-tune"),"tune",act("fine"),false)}
    if v.fine then
        table.insert(right,label("tint-title",ui.tr("bead.colors.background-tint"),11))
        table.insert(right,kit.ChipRow{id="tint",per_row=2,on_change="colors.do",change_arg=function(id) return "bg:"..id end,items={
            {id="accent",label=ui.tr("bead.colors.tint-accent"),selected=not v.tint_wallpaper},
            {id="wallpaper",label=ui.tr("bead.colors.tint-wallpaper"),selected=v.tint_wallpaper,enabled=v.tint_wallpaper_enabled}}})
        table.insert(right,kit.Paragraph{id="tint-note",text=v.tint_wallpaper and ui.tr("bead.colors.tint-wallpaper-note") or ui.tr("bead.colors.tint-accent-note"),
            width=COL2,font_size=px(9),color="onSurfaceVariant"})
        table.insert(right,label("brightness-title",ui.tr("bead.colors.brightness"),10))
        table.insert(right,kit.Slider{id="brightness",width=COL2,min=-.15,max=.15,step=.01,value=v.brightness,label=ui.tr("bead.colors.brightness"),
            on_change="colors.do",change_arg=function(n) return string.format("brightness:%.3f",n) end})
        table.insert(right,label("strength-title",ui.tr("bead.colors.strength"),10))
        table.insert(right,kit.Slider{id="strength",width=COL2,min=0,max=1.6,step=.05,value=v.saturation,label=ui.tr("bead.colors.strength"),
            on_change="colors.do",change_arg=function(n) return string.format("saturation:%.3f",n) end})
        table.insert(right,button("time-tint",ui.tr("bead.colors.time-shift"),nil,act("tint"),v.time_tint))
        table.insert(right,button("ws-colors",ui.tr("bead.colors.workspace-chips"),nil,act("wsc"),v.workspace_colors))
        table.insert(right,button("reset",ui.tr("bead.colors.reset"),nil,act("reset"),false))
    end
    if split then
        return ui.Row{id="studio",width="fill",gap=20,shrink=0,ui.Column(left),ui.Column(right)}
    end
    return ui.Column{id="studio",width="fill",gap=20,shrink=0,ui.Column(left),ui.Column(right)}
end
local function compare(v)
    local w=math.max(300,(CANVAS-12)/2)
    local rows,row={},nil
    for i,c in ipairs(v.compare) do
        if not row or #row>=2 then row={};table.insert(rows,row) end
        table.insert(row,ui.Column{id="cmp-"..c.id,width=w,gap=8,shrink=0,
            label("name",c.label,11),
            ui.Text{id="contrast",text=c.contrast,visible=c.contrast~="",width="fill",height=c.contrast~="" and 14 or 0,font_size=px(9),color="onSurfaceVariant"},
            kit.PreviewCard{id="card",width=w,height=math.min(260,w*.6),colors=c.colors,image=v.has_photo and "colors-card" or "",
                aria_label=c.label,labels={brand=ui.tr("bead.colors.card-brand"),files=ui.tr("bead.colors.card-files"),
                    lines={ui.tr("bead.colors.card-documents"),ui.tr("bead.colors.card-pictures"),ui.tr("bead.colors.card-projects")},open=ui.tr("bead.colors.card-open"),
                    build=ui.tr("bead.colors.card-build"),ready=ui.tr("bead.colors.card-ready"),warning=ui.tr("bead.colors.card-warning"),
                    message=ui.tr("bead.colors.card-message"),message_detail=ui.tr("bead.colors.card-message-detail")}}})
    end
    local out={id="compare",width="fill",gap=14,shrink=0,label("compare-title",ui.tr("bead.colors.compare-styles"),14)}
    for i,r in ipairs(rows) do
        r.id="pair-"..i;r.gap=12;r.shrink=0
        table.insert(out,ui.Row(r))
    end
    return ui.Column(out)
end
local function access(v)
    local chips={}
    for i,c in ipairs(v.vision) do chips[i]={id=c.id,label=c.label,selected=c.selected} end
    local out={id="access",width="fill",gap=12,shrink=0,
        label("access-title",ui.tr("bead.colors.access-title"),16),
        ui.Text{id="contrast",text=v.contrast,width="fill",height=18,font_size=px(10),color="onSurfaceVariant"},
        kit.Paragraph{id="coverage",text=ui.tr("bead.colors.access-coverage"),width=CANVAS,font_size=px(10),color="onSurfaceVariant"},
        kit.ChipRow{id="vision",items=chips,per_row=4,on_change="colors.do",change_arg=function(id) return "vision:"..id end},
        kit.PreviewCard{id="card",width=math.min(CANVAS,620),height=300,colors=v.vision_colors,image=v.has_photo and "colors-card" or "",
            aria_label=ui.tr("bead.colors.preview-aria"),labels={brand=ui.tr("bead.colors.card-brand"),files=ui.tr("bead.colors.card-files"),
                lines={ui.tr("bead.colors.card-documents"),ui.tr("bead.colors.card-pictures"),ui.tr("bead.colors.card-projects")},open=ui.tr("bead.colors.card-open"),
                build=ui.tr("bead.colors.card-build"),ready=ui.tr("bead.colors.card-ready"),warning=ui.tr("bead.colors.card-warning"),
                message=ui.tr("bead.colors.card-message"),message_detail=ui.tr("bead.colors.card-message-detail")}}}
    for _,w in ipairs(v.warnings) do table.insert(out,ui.Text{id=w.id,text=w.text,width="fill",height=18,font_size=px(10),color="onSurfaceVariant"}) end
    for _,r in ipairs(v.rows) do table.insert(out,ui.Text{id=r.id,text=r.text,width="fill",height=16,font_size=px(9),color="onSurfaceVariant"}) end
    return ui.Column(out)
end
local function canvas(v)
    local pages={studio=studio,compare=compare,accessibility=access}
    return ui.Scroll{id="canvas",scroll=ui.bind("colors.scroll"),grow=1,
        ui.Column{id="body",width="fill",gap=14,
            label("title",v.title,20,"onSurface",32),
            ui.Text{id="subtitle",text=v.subtitle,width="fill",height=16,font_size=px(10),color="onSurfaceVariant"},
            (pages[page] or studio)(v)}}
end
local function footer(v)
    local apply=v.applied and ui.tr("bead.colors.in-use") or ui.tr("bead.colors.apply")
    return ui.Column{id="footer",width="fill",height=80,shrink=0,gap=sp.compact,
        ui.Text{id="status",text=v.status,width="fill",height=20,font_size=px(10),color=v.status_error and "error" or "onSurfaceVariant"},
        ui.Row{id="actions",width="fill",height=36,gap=sp.sm,align="center",
            button("export",ui.tr("bead.colors.export"),"upload",v.can_export and act("export") or nil,false),
            ui.Box{id="space",grow=1,height=1},
            button("apply",apply,"check",v.can_apply and act("apply") or nil,true)}}
end
return ui.Column{id="colors",width="100%",height="100%",fill="surface",
    ui.Repeat{id="top",model=ui.bind("colors.view"),key="id",render=header},
    ui.Box{id="rule",width="100%",height=1,shrink=0,fill=ui.alpha("outline",.35)},
    ui.Column{id="content",grow=1,width="fill",padding=MARGIN,gap=12,
        ui.Row{id="main",width="fill",grow=1,gap=MARGIN,rail(),
            ui.Column{id="canvas-col",grow=1,height="100%",ui.Repeat{id="page",model=ui.bind("colors.view"),key="id",render=canvas}}},
        ui.Repeat{id="bottom",model=ui.bind("colors.view"),key="id",render=footer}}}
