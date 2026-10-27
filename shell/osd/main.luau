-- SPDX-License-Identifier: GPL-3.0-only
-- On-screen display (B0051): the QML pill (`NacreOsdPill`) over the daemon's `osd` events. The host
-- projects `osd.kind/value/muted/label/device`; kinds.luau holds what each kind looks like.
local ui=require("bead.ui")
local kit=require("bead.kit")
local kinds=require("./kinds")
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local W,ICON_X,TEXT_X,EDGE=336,19,60,24
local TITLE,TITLE_H,DETAIL_H,VALUE_W=(kit.tokens.text.caption+1)*4/3,19,16,64
local ICON_SIZE=(kit.tokens.icons.xlarge-2)*4/3
local ICON_H=math.ceil(ICON_SIZE*1.4)
local kind,value,muted,label,device=get("osd.kind"),get("osd.value"),get("osd.muted"),get("osd.label"),get("osd.device")
local spec=kinds[kind] or kinds.volume
value=math.clamp(value,0,1)
local function glyph(g)
    if g.muted and muted then return g.muted end
    for _,step in g.below or {} do if value<step[1] then return step[2] end end
    local hay=string.lower(label.." "..device)
    for _,m in g.match or {} do for _,p in m.patterns do if string.find(hay,p,1,true) then return m.glyph end end end
    return g.by_label and g.by_label[label] or g.default
end
local function detail(d)
    if d.state then return ui.tr(value>0 and "bead.osd.on" or "bead.osd.off") end
    if d.fixed then return ui.tr(d.fixed) end
    if d.by_label and d.by_label[label] then return ui.tr(d.by_label[label]) end
    if d.label and label~="" then return label end
    if d.fallback then return ui.tr(d.fallback) end
    return ""
end
local level,text=spec.level==true,detail(spec.detail)
local fill_value=level and ui.animate("osd-level",value,0.13,"out-cubic") or value
local dim=muted or (spec.toggle==true and value<=0)
local percent=muted and ui.tr("bead.osd.muted") or string.format("%d%%",math.floor(value*100+.5))
local H=level and (text~="" and 76 or 60) or 68
local titleTop=(level and 12 or 14)-kit.tokens.spacing.hairline
local TRACK_W=W-TEXT_X-EDGE
return kit.Panel{edge="bottom",id="osd",width=W,height=H,on_activate=ui.action("notifications.osd",true),open=ui.bind("panel.open"),duration=.150,slide_distance=4,
    fill="surface",radius=H/2,border_width=1,border_color=ui.alpha("outline",.28),padding=0,
    ui.Text{id="icon",position="absolute",left=ICON_X,top=(H-ICON_H)/2,width=30,height=ICON_H,text=glyph(spec.glyph),family="Material Symbols Rounded",font_size=ICON_SIZE,color=dim and "onSurfaceVariant" or "primary"},
    ui.Text{id="title",position="absolute",left=TEXT_X,top=titleTop,width=W-TEXT_X-EDGE-VALUE_W,height=TITLE_H,family="IBM Plex Sans",text=ui.tr(spec.title),font_size=TITLE,font_weight=500},
    ui.Text{id="value",position="absolute",left=W-EDGE-VALUE_W,top=titleTop,width=VALUE_W,height=TITLE_H,family="IBM Plex Sans",text=level and percent or "",text_align="right",font_size=TITLE,color="onSurfaceVariant"},
    ui.Text{id="detail",position="absolute",left=TEXT_X,top=titleTop+TITLE_H+2,width=W-TEXT_X-EDGE,height=DETAIL_H,family="IBM Plex Sans",text=text,font_size=12,color="onSurfaceVariant",visible=text~=""},
    ui.Box{id="track",position="absolute",left=TEXT_X,top=H-18-kit.tokens.spacing.hairline,width=TRACK_W,height=5,radius=2.5,fill="surfaceContainer",visible=level,clip_children=true,
        ui.Box{id="fill-window",position="absolute",left=0,top=0,width=TRACK_W*fill_value,height=5,clip_children=true,fill="transparent",
            ui.Image{id="fill",position="absolute",left=0,top=0,path="osd/assets/progress.themed.svg",width=TRACK_W,height=5,visible=not muted},
            ui.Box{id="muted-fill",position="absolute",left=0,top=0,width=TRACK_W,height=5,radius=2.5,fill=ui.alpha("primary",.35),visible=muted},
            ui.Box{id="fill-cap",position="absolute",right=0,top=0,width=5,height=5,radius=2.5,fill=muted and ui.alpha("primary",.35) or "secondary"}},
    }
}
