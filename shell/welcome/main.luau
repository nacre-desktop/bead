-- SPDX-License-Identifier: GPL-3.0-only
-- Welcome (B0059): the first-run tour as an on-demand window composed from kit components.
-- Pages, app tiles, help and links are data in `catalog.luau`; the adapter only supplies the window
-- size, which apps this desktop can open and the shortcut rows. Text blocks wrap inside their
-- box, so heights come from `textH`, an estimate from the character count and the content width.
local ui=require("bead.ui")
local kit=require("bead.kit")
local catalog=require("./catalog")
local T=kit.tokens
local sp,tx,rd,ic=T.spacing,T.text,T.radii,T.icons
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local function px(pt) return pt*4/3 end
local function act(arg) return ui.action("welcome.do",arg) end
local function go(name) return ui.action("go",name) end

local page,guide=get("welcome.step"),get("welcome.guide")
local W,H=get("welcome.width"),get("welcome.height")
-- Content box: the window minus 24 px side margins; the canvas also loses the header and footer.
local CW=W-2*sp.xxl
local CH=H-176

local function wrapped(s,size,width)
    return math.max(1,math.ceil(#s/math.max(1,math.floor(width/(size*0.48)))))
end
local function textH(s,size,width)
    return math.ceil(wrapped(s,size,width)*size*1.4)+2
end
-- Visual height of a block (the box is taller by the line pitch): one line is about 1.25 em in QML.
local function inkH(s,size,width)
    local n=wrapped(s,size,width)
    return n==1 and math.ceil(size*1.25) or math.ceil(n*size*1.4)
end
-- A text block wrapped at `o.width` (the content width by default).
local function para(id,s,pt,o)
    o=o or {}
    local size=px(pt)
    return ui.Text{id=id,text=s,font_size=size,height=textH(s,size,o.width or CW),shrink=0,
        color=o.color or "onSurface",text_align=o.align,family=o.family}
end
local function muted(id,s,pt,o)
    o=o or {}
    o.color="onSurfaceVariant"
    return para(id,s,pt,o)
end
local function rule(id,opacity)
    return ui.Box{id=id,width="fill",height=1,shrink=0,fill=ui.alpha("outline",opacity)}
end
-- Rounded text button (QML `ActionButton`): a pill; `selected` fills with the primary role, a nil
-- `on` dims it.
local function pillWidth(label,compact,pt)
    return math.ceil(#label*px(pt or tx.body)*0.55+(compact and sp.xl or sp.huge))
end
local function pill(o)
    local size=px(o.pt or tx.body)
    local lh=math.ceil(size*1.4)
    local h=34
    local armed=o.on~=nil
    local ink=o.selected and "onPrimary" or (armed and "onSurface" or "onSurfaceVariant")
    return kit.Button{id=o.id,width=pillWidth(o.label,o.compact,o.pt),height=h,shrink=0,padding=0,
        padding_top=math.floor((h-lh)/2),radius=rd.full,justify="center",align="center",
        position=o.position,left=o.left,right=o.right,top=o.top,
        fill=o.selected and "primary" or "surfaceContainerHigh",
        hover_fill=o.selected and "primary" or "surfaceContainerHighest",aria_label=o.aria or o.label,on_activate=o.on,
        ui.Text{id=o.id.."-label",text=o.label,height=lh,grow=1,font_size=size,text_align="center",color=ink}}
end
local function logo(id,size)
    return ui.Image{id=id,path="welcome/assets/nacre-logo.svg",width=size,height=size,shrink=0,aria_label=ui.tr("bead.welcome.hero-name")}
end
local function column(id,gap,...)
    return ui.Column{id=id,width="fill",shrink=0,gap=gap,...}
end
local function wrapRow(id,gap,...)
    return ui.Row{id=id,width="fill",height=34,shrink=0,gap=gap,...}
end

-- App tile: glyph, title, detail and an Open / Setup guide / Not installed button.
local function appRow(app)
    local available=get(app.needs)
    local optional=app.optional and not available
    local stacked=CW<640
    local title,detail=ui.tr(app.title),ui.tr(app.detail)
    if optional then detail=ui.tr("bead.welcome.optional-detail",{detail=detail}) end
    local label=optional and ui.tr("bead.welcome.button-setup-guide")
        or (available and ui.tr("bead.welcome.button-open") or ui.tr("bead.welcome.button-not-installed"))
    local action
    if optional then action=ui.action("guide",app.id)
    elseif available then action=act("route:"..app.route) end
    local bw=pillWidth(label)
    local textW=math.max(80,CW-76-(stacked and 18 or bw+40))
    local tsize,dsize=px(13),px(10.5)
    local th,dh=textH(title,tsize,textW),textH(detail,dsize,textW)
    local rowH=math.max(76,16+inkH(title,tsize,textW)+5+inkH(detail,dsize,textW)+(stacked and 34+12+16 or 18))
    return ui.Column{id="app-"..app.id,width="fill",height=rowH,shrink=0,radius=12,fill="surfaceContainer",
        ui.Text{id="glyph",position="absolute",left=18,top=20,width=ic.hero,height=ic.hero,text=app.icon,
            family="Material Symbols Rounded",font_size=px(19),color=optional and ui.alpha("primary",.4) or "primary"},
        ui.Text{id="title",position="absolute",left=76,top=16,width=textW,height=th,text=title,font_size=tsize,
            color=optional and "onSurfaceVariant" or "onSurface"},
        ui.Text{id="detail",position="absolute",left=76,top=16+inkH(title,tsize,textW)+5,width=textW,height=dh,text=detail,font_size=dsize,color="onSurfaceVariant"},
        pill{id="open",label=label,on=action,position="absolute",right=18,top=stacked and 16+inkH(title,tsize,textW)+5+inkH(detail,dsize,textW)+12 or (rowH-34)/2}}
end

local pages={}
function pages.welcome()
    local logoSize=math.max(180,math.min(320,CH*.38))
    local nameSize=math.max(30,math.min(46,CH*.065))
    local tagSize=math.max(13,math.min(17,CW/100))
    return ui.Column{id="hero",width="fill",shrink=0,align="center",gap=sp.xxl,
        ui.Box{id="hero-top",width=1,height=math.max(18,CH*.12),shrink=0},
        logo("hero-logo",logoSize),
        para("hero-name",ui.tr("bead.welcome.hero-name"),nameSize,{align="center"}),
        para("hero-tagline",ui.tr("bead.welcome.hero-tagline"),tagSize,{align="center"}),
        muted("hero-tour",ui.tr("bead.welcome.hero-tour"),11,{align="center"})}
end
function pages.colors()
    local cells={}
    for _,item in ipairs(catalog.roles) do
        table.insert(cells,ui.Row{id="role-"..item.role,grow=1,height=34,gap=10,
            ui.Box{id="swatch",width=34,height=34,shrink=0,radius=9,fill=item.role,border_width=1,border_color="outline"},
            ui.Column{id="role-name",grow=1,height=34,justify="center",
                ui.Text{id="label",text=ui.tr(item.label),height=math.ceil(px(10)*1.4),font_size=px(10),color="onSurface"}}})
    end
    local can=get("welcome.can.colors")
    return column("colors",12,
        para("colors-title",ui.tr("bead.welcome.colors-title"),25),
        muted("colors-intro",ui.tr("bead.welcome.colors-intro"),12),
        ui.Row{id="roles",width="fill",height=34,shrink=0,gap=12,table.unpack(cells)},
        muted("colors-more",ui.tr("bead.welcome.colors-more"),10),
        ui.Row{id="colors-actions",width="fill",height=34,shrink=0,
            pill{id="colors-open",label=can and ui.tr("bead.welcome.colors-open") or ui.tr("bead.welcome.button-not-installed"),
                on=can and act("route:colors") or nil}})
end
function pages.shortcuts()
    local state=get("welcome.shortcuts.state")
    local note=state=="unavailable" and ui.tr("bead.welcome.shortcuts-unavailable")
        or (state=="empty" and ui.tr("bead.welcome.shortcuts-empty") or (state=="loading" and ui.tr("bead.welcome.shortcuts-loading") or ""))
    local titleW=CW*.43-20
    local keyW=CW*.57-28
    local size=px(10.5)
    local head={para("shortcuts-title",ui.tr("bead.welcome.shortcuts-title"),25),
        muted("shortcuts-intro",ui.tr("bead.welcome.shortcuts-intro"),11)}
    if note~="" then table.insert(head,muted("shortcuts-state",note,10.5)) end
    table.insert(head,ui.Column{id="shortcut-list",width="fill",shrink=0,gap=8,
            ui.Repeat{id="shortcut-rows",model=ui.bind("welcome.shortcuts"),key="id",render=function(item)
                local height=math.max(58,math.max(textH(item.title,size,titleW),textH(item.key,size,keyW))+24)
                return ui.Row{id="shortcut",width="fill",height=height,shrink=0,gap=6,radius=10,fill="surfaceContainer",
                    padding_left=14,padding_right=14,padding_top=12,
                    ui.Text{id="title",width=titleW,height=height-24,text=item.title,font_size=size,color="onSurface"},
                    ui.Text{id="key",width=1,grow=1,height=height-24,text=item.key,font_size=size,family="IBM Plex Mono",color="primary"}}
            end}})
    table.insert(head,muted("shortcuts-note",ui.tr("bead.welcome.shortcuts-note"),10))
    return column("shortcuts",14,table.unpack(head))
end
function pages.apps()
    local items={}
    if guide~="" and catalog.guides[guide] then
        local g=catalog.guides[guide]
        local title,detail,setup,status=ui.tr(g.title),ui.tr(g.detail),ui.tr(g.setup),ui.tr(g.status)
        local w=CW-32
        local h=textH(title,px(16),w)+10+textH(detail,px(12),w)+10+textH(setup,px(12),w)+10+textH(status,px(10.5),w)+10+34
        table.insert(items,ui.Column{id="guide",width="fill",height=h+32,shrink=0,radius=12,fill="surfaceContainer",padding=16,gap=10,
            para("guide-title",title,16,{width=w}),para("guide-detail",detail,12,{width=w}),
            muted("guide-setup",setup,12,{width=w}),muted("guide-status",status,10.5,{width=w}),
            ui.Row{id="guide-actions",width="fill",height=34,shrink=0,
                pill{id="guide-back",label=ui.tr("bead.welcome.guide-back"),on=ui.action("guide","")}}})
    end
    for _,app in ipairs(catalog.apps) do table.insert(items,appRow(app)) end
    table.insert(items,muted("apps-note",ui.tr("bead.welcome.apps-note"),10.5))
    return column("apps",14,
        para("apps-title",ui.tr("bead.welcome.apps-title"),23),
        muted("apps-intro",ui.tr("bead.welcome.apps-intro"),12),
        table.unpack(items))
end
function pages.ready()
    return ui.Column{id="ready",width="fill",shrink=0,align="center",gap=18,
        ui.Box{id="ready-top",width=1,height=math.max(0,(CH-340)/2),shrink=0},
        logo("ready-logo",120),
        para("ready-title",ui.tr("bead.welcome.ready-title"),28,{align="center"}),
        muted("ready-note",ui.tr("bead.welcome.ready-note"),10.5,{align="center"}),
        pill{id="ready-help",label=ui.tr("bead.welcome.ready-help"),compact=true,pt=tx.caption,on=go("help")}}
end
function pages.help()
    local rows={para("help-title",ui.tr("bead.welcome.help-title"),23)}
    for index,item in ipairs(catalog.help) do
        table.insert(rows,column("help-"..index,2,
            para("title",ui.tr(item.title),13),muted("detail",ui.tr(item.detail),11),rule("rule",.2)))
    end
    local routes,links={},{}
    for index,item in ipairs(catalog.helpRoutes) do
        table.insert(routes,pill{id="route-"..index,label=ui.tr(item.label),on=act("route:"..item.route)})
    end
    for index,item in ipairs(catalog.links) do
        table.insert(links,pill{id="link-"..index,label=ui.tr(item.label),on=act("link:"..item.link)})
    end
    table.insert(rows,wrapRow("help-routes",8,table.unpack(routes)))
    table.insert(rows,para("help-online",ui.tr("bead.welcome.help-online"),15))
    table.insert(rows,wrapRow("help-links",8,table.unpack(links)))
    table.insert(rows,muted("help-about",ui.tr("bead.welcome.help-about"),10.5))
    return column("help",14,table.unpack(rows))
end

-- Back / Skip tour, step dots, Next / Start using Nacre.
local steps=catalog.steps
local index=0
for i,name in ipairs(steps) do if name==page then index=i end end
local backLabel=(index==1 and page~="help") and ui.tr("bead.welcome.skip-tour") or ui.tr("bead.welcome.back")
local backAction
if page=="help" then backAction=go("ready")
elseif index>1 then backAction=go(steps[index-1])
else backAction=act("close") end
local nextLabel,nextAction=ui.tr("bead.welcome.next"),nil
if page=="ready" then nextLabel,nextAction=ui.tr("bead.welcome.start"),act("close")
elseif page=="help" then nextLabel,nextAction=ui.tr("bead.welcome.back-to-finish"),go("ready")
else nextAction=go(steps[index+1] or "ready") end
local dots={}
for i,name in ipairs(steps) do
    table.insert(dots,kit.Button{id="step-"..name,width=10,height=10,shrink=0,padding=0,radius=5,fill="transparent",
        hover_fill="transparent",aria_label=ui.tr("bead.welcome.step",{number=i,name=ui.tr(catalog.sections[name])}),on_activate=go(name),
        ui.Box{id="dot",width=10,height=10,radius=5,fill=name==page and "primary" or "outline"}})
end
local footer=ui.Column{id="footer",width="fill",height=64,shrink=0,
    rule("footer-rule",.3),
    pill{id="back",label=backLabel,compact=true,pt=tx.caption,on=backAction,position="absolute",left=0,top=15},
    ui.Row{id="dots",position="absolute",left=(CW-(#dots*10+(#dots-1)*10))/2,top=27,width=#dots*10+(#dots-1)*10,height=10,gap=10,table.unpack(dots)},
    pill{id="next",label=nextLabel,selected=true,on=nextAction,position="absolute",right=0,top=15}}

local function section()
    return catalog.sections[page] and ui.tr(catalog.sections[page]) or ""
end
local header=ui.Column{id="header",width="fill",height=64,shrink=0,
    ui.Row{id="header-row",width="fill",height=63,align="center",padding_left=sp.xl,padding_right=sp.xl,gap=sp.lg,
        ui.Image{id="header-logo",path="welcome/assets/nacre-logo.svg",width=ic.hero,height=ic.hero,shrink=0},
        ui.Text{id="header-title",text=ui.tr("bead.welcome.title"),height=math.ceil(px(tx.title)*1.4),font_size=px(tx.title),width=1,grow=1,color="onSurface"},
        ui.Text{id="header-section",text=section(),width=240,height=math.ceil(px(tx.caption)*1.4),font_size=px(tx.caption),
            text_align="right",color="onSurfaceVariant"}},
    rule("header-rule",.35)}

return ui.Column{id="welcome",width="100%",height="100%",fill="surface",
    header,
    ui.Column{id="body",width="fill",grow=1,padding_left=sp.xxl,padding_right=sp.xxl,padding_top=sp.md+sp.xs,padding_bottom=sp.md+sp.xs,
        ui.Scroll{id="canvas",scroll=ui.bind("welcome.scroll"),grow=1,(pages[page] or pages.welcome)()},
        ui.Box{id="gap",width=1,height=sp.md+sp.xs,shrink=0},
        footer}}
