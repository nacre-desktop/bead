-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local function textHeight(text,size,maximum)
    if text=="" then return 0 end
    local lines=0
    local columns=math.max(1,math.floor(372/(size*.55)))
    for line in (text.."\n"):gmatch("(.-)\n") do lines+=math.max(1,math.ceil((utf8.len(line) or #line)/columns)) end
    return math.min(maximum,lines)*size*1.4
end
local function actionRows(n)
    local rows={};local row={};local width=0
    for i,a in n.actions do
        -- The private owner marks its default as "id|default"; the daemon rows already omit it.
        if a.id==n.id.."|default" then continue end
        local w=math.min(372,math.max(56,(utf8.len(a.label) or #a.label)*7+24))
        if #row>0 and width+6+w>372 then table.insert(rows,row);row={};width=0 end
        table.insert(row,{index=i,action=a,width=w});width+=w+(#row>1 and 6 or 0)
    end
    if #row>0 then table.insert(rows,row) end
    return rows
end
local function footerHeight(n)
    if not n.expanded then return 0 end
    local rows=#actionRows(n)
    return rows>0 and rows*34+(rows-1)*6 or 0
end
local function noticeHeight(n)
    local body=textHeight(n.body,13.333,n.expanded and 12 or 2)
    local footer=footerHeight(n)
    return 28+30+9+textHeight(n.summary,16,n.expanded and 6 or 2)+(body>0 and 9+body or 0)+(footer>0 and 9+footer or 0)
end
local function notice(n)
    local hasDetails=n.body~="" or #actionRows(n)>0
    local buttons={id="actions",gap=6,height=footerHeight(n),visible=n.expanded and #actionRows(n)>0}
    for i,items in actionRows(n) do
        local row={id="actions-row-"..i,gap=6,height=34}
        for j,item in items do row[j]=kit.Button{id="action-"..item.index,width=item.width,height=34,family="IBM Plex Sans",font_size=kit.tokens.text.caption*4/3,fill="surfaceContainerHigh",text=item.action.label,on_activate=ui.action("notifications.action",item.action.ref or item.action.id)} end
        buttons[i]=ui.Row(row)
    end
    return kit.Card{id="notice",height=noticeHeight(n),radius=14,padding=14,fill="surfaceContainer",on_activate=ui.action("notifications.card",n.key or n.id),aria_label=n.app..": "..n.summary,
        border_width=1,border_color=n.urgency==2 and "error" or ui.alpha("outlineVariant",.45),gap=9,
        ui.Row{height=30,gap=10,
            ui.Image{icon=n.icon~="" and n.icon or "dialog-information",width=28,height=28,aria_label=n.app},
            ui.Column{grow=1,gap=0,justify="center",ui.Text{family="IBM Plex Sans",text=n.app,font_size=13.333,height=16},ui.Text{family="IBM Plex Sans",text=n.time_str or "now",font_size=10.667,height=13,color="onSurfaceVariant"}},
            kit.IconButton{id="expand",visible=hasDetails,text=n.expanded and "expand_less" or "expand_more",width=30,height=30,radius=15,fill="transparent",aria_label=n.expanded and "Collapse notification" or "Expand notification",on_activate=ui.action("notifications.details",n.key or n.id)},
            kit.IconButton{id="dismiss",text="close",width=30,height=30,radius=15,fill="transparent",aria_label="Dismiss notification",on_activate=ui.action("notifications.dismiss",n.key or n.id)},
        },
        ui.Text{family="IBM Plex Sans",text=n.summary,font_size=16,height=textHeight(n.summary,16,n.expanded and 6 or 2)},
        ui.Text{family="IBM Plex Sans",text=n.body,font_size=13.333,color="onSurfaceVariant",height=textHeight(n.body,13.333,n.expanded and 12 or 2),visible=n.body~=""},
        ui.Column(buttons),
    }
end
local rows=ui.resolve_for_check(ui.bind("notifications.rows"))
local height=math.max(0,#rows-1)*8
for _,n in rows do height+=noticeHeight(n) end
return ui.Column{edge="right",id="notifications",width=400,height=math.max(1,math.min(720,height)),fill="transparent",radius=0,
    open=ui.bind("notifications.open"),duration=.160,slide_distance=0,
    ui.Scroll{id="stream",grow=1,scroll=ui.bind("notifications.scroll.y"),ui.Column{shrink=0,gap=8,ui.Repeat{id="notices",model=ui.bind("notifications.rows"),key="id",render=notice}}},
}
