-- SPDX-License-Identifier: GPL-3.0-only
-- AI sidebar (B0043) composed from kit components. Geometry (edge, button height,
-- tab and content tops, footer positions) is projected from the size scale by the
-- adapter, so this file holds no pixel literals tied to one setup.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local px=function(pt) return pt*4/3 end
local BODY,CAPTION,TITLE=px(T.text.body),px(T.text.caption),px(T.text.title)
local H=get("chat.height")
local section=get("chat.section")
local E,BTN,INNER,TABS,TOP=get("chat.edge"),get("chat.button"),get("chat.inner"),get("chat.tabs"),get("chat.top")
local W=INNER+2*E
local ICON=px(T.icons.large)
local kids={}
local function add(node) table.insert(kids,node) end
-- Rounded button with an optional Material icon and label; no action means inert.
local function button(id,x,y,w,h,label,icon,action,selected,size,aria)
    local ink=selected and "onPrimary" or (action and "onSurface" or "onSurfaceVariant")
    local inner=w-2*T.spacing.sm
    local row={}
    if icon~="" then
        table.insert(row,ui.Text{id="icon",text=icon,family="Material Symbols Rounded",width=ICON+2,height=24,font_size=ICON,shrink=0,color=ink})
        inner=inner-ICON-2-T.spacing.xs
    end
    if label~="" then
        table.insert(row,ui.Text{id="label",text=label,width=inner,height=24,font_size=size or BODY,shrink=0,
            text_align=icon~="" and "left" or "center",color=ink})
    end
    return kit.Button{id=id,position="absolute",left=x,top=y,width=w,height=h,padding=0,radius=T.spacing.md,
        fill=selected and "primary" or "surfaceContainer",aria_label=aria or (label~="" and label or icon),on_activate=action,
        ui.Row{id=id.."-row",width=w,height=h,gap=T.spacing.xs,align="center",justify="center",table.unpack(row)}}
end
local TAB_H=BTN+4*T.spacing.xs
local function tab(id,x,w,label,icon)
    local selected=section==id
    local ink=selected and "primary" or "onSurfaceVariant"
    local row={ui.Text{id="icon",text=icon,family="Material Symbols Rounded",width=ICON+2,height=24,font_size=ICON,shrink=0,color=ink}}
    if label~="" then table.insert(row,ui.Text{id="label",text=label,width=w-2*T.spacing.xxl,height=24,font_size=BODY,shrink=0,color=ink}) end
    add(kit.Button{id="tab-"..id,position="absolute",left=x,top=TABS,width=w,height=TAB_H,padding=0,radius=T.spacing.md,
        fill=selected and ui.alpha("primary",0.12) or "transparent",aria_label=label~="" and label or ui.tr("bead.panel.chat-tab-settings"),
        on_activate=(not selected) and ui.action("chat.do","section:"..id) or nil,
        ui.Row{id="tab-"..id.."-row",width=w,height=TAB_H,gap=T.spacing.xs,align="center",justify="center",table.unpack(row)}})
end
add(ui.Text{id="logo",position="absolute",left=E,top=E+T.spacing.hairline,width=T.icons.hero,height=T.icons.hero,text="auto_awesome",
    family="Material Symbols Rounded",font_size=T.icons.hero*.8,color="primary"})
add(ui.Text{id="title",position="absolute",left=E+T.icons.hero+T.spacing.sm+T.spacing.xs,top=E+T.spacing.xs-1,width=W/2,height=T.icons.hero,
    text=ui.tr("bead.panel.chat-title"),font_size=T.text.headline*4/3-1,color="primary"})
local TW=(INNER-2*T.spacing.sm)/3.7
tab("chat",E,TW,ui.tr("bead.panel.chat-tab-chat"),"forum")
tab("chats",E+TW+T.spacing.sm,TW*1.07,ui.tr("bead.panel.chat-tab-chats"),"history")
tab("settings",E+TW*2.07+2*T.spacing.sm,TAB_H+T.spacing.xs,"","settings")

if section=="chat" then
    add(button("new",E,TOP,INNER*.297,BTN,ui.tr("bead.panel.chat-new"),"add_comment",get("chat.can_new") and ui.action("chat.do","new") or nil,false,CAPTION+1))
    add(button("workspace",E+INNER*.297+T.spacing.sm,TOP,INNER*.342,BTN,ui.tr("bead.panel.chat-workspace"),"open_in_new",
        get("chat.can_workspace") and ui.action("chat.do","workspace") or nil,false,CAPTION+1))
    add(ui.Text{id="heading",position="absolute",left=E,top=TOP+BTN+T.spacing.sm-1,width=INNER,height=CAPTION*1.5,text=ui.bind("chat.heading"),
        font_size=CAPTION,color="onSurfaceVariant"})
    local TT,TH=get("chat.transcript_top"),get("chat.transcript_height")
    add(kit.Transcript{id="transcript",position="absolute",left=E,top=TT,width=INNER,height=TH,scroll=ui.bind("chat.scroll"),
        cells=ui.bind("chat.cells"),extent=get("chat.extent"),on_action="chat.do"})
    if get("chat.empty") then
        add(ui.Text{id="empty",position="absolute",left=E,top=TT+TH/2-12,width=INNER,height=24,text=ui.tr("bead.panel.chat-empty"),
            font_size=BODY,text_align="center",color="onSurfaceVariant"})
    end
    if get("chat.status_height")>0 then
        add(ui.Text{id="status",position="absolute",left=E,top=get("chat.status_top"),width=INNER,height=get("chat.status_height"),
            text=ui.bind("chat.status"),font_size=CAPTION,color=get("chat.status_error") and "error" or "onSurfaceVariant"})
    end
    if get("chat.has_question") then
        local can_reply=get("chat.can_reply")
        local RW=INNER-T.spacing.sm
        add(ui.Scroll{id="questions",scroll=ui.bind("chat.qscroll"),position="absolute",left=E,top=get("chat.question_top"),width=INNER,height=get("chat.question_height"),
            ui.Column{id="question-rows",width=RW,gap=T.spacing.xs+1,
                ui.Repeat{id="rows",model=ui.bind("chat.question_rows"),key="id",render=function(item)
                    if item.k=="text" then
                        return ui.Text{id="prompt",text=item.t,width=RW,height=item.h,font_size=BODY,color="onSurface"}
                    elseif item.k=="option" then
                        return kit.Button{id="option",width=RW,height=item.h,padding=0,padding_top=T.spacing.xs+1,radius=T.spacing.compact,
                            fill=item.sel and "primary" or "surfaceContainerHigh",aria_label=item.t,on_activate=ui.action("chat.do",item.a),
                            ui.Text{id="label",text=item.t,width=RW-2*T.spacing.sm,height=24,font_size=BODY-2,text_align="center",
                                color=item.sel and "onPrimary" or "onSurface"}}
                    elseif item.k=="input" then
                        return kit.TextInput{id="answer",text=ui.bind("chat.answer."..item.i),placeholder=ui.tr("bead.panel.chat-your-answer"),
                            width=RW,height=item.h,padding=T.spacing.compact,padding_top=T.spacing.xs+1,padding_bottom=0,radius=T.spacing.compact,font_size=BODY-1,fill="surfaceContainerHigh"}
                    end
                    return kit.Button{id="reply",width=RW,height=item.h,padding=0,padding_top=T.spacing.sm-2,radius=T.spacing.md,
                        fill=can_reply and "primary" or "surfaceContainer",aria_label=ui.tr("bead.panel.chat-reply"),
                        on_activate=can_reply and ui.action("chat.do","reply") or nil,
                        ui.Text{id="label",text=ui.tr("bead.panel.chat-reply"),width=RW-2*T.spacing.sm,height=24,font_size=BODY-1,text_align="center",
                            color=can_reply and "onPrimary" or "onSurfaceVariant"}}
                end}}})
    end
    add(kit.Composer{id="composer",position="absolute",left=E,top=get("chat.composer_top"),width=INNER,height=get("chat.composer_height"),
        font_size=BODY,text=ui.bind("chat.draft"),placeholder=get("chat.placeholder"),aria_label=get("chat.placeholder"),
        on_activate=ui.action("chat.do","send")})
    local AT=get("chat.actions_top")
    local busy=get("chat.busy")
    add(button("send",E,AT,INNER*.228,BTN,ui.tr("bead.panel.chat-send"),"arrow_upward",get("chat.can_send") and ui.action("chat.do","send") or nil,get("chat.can_send"),CAPTION+1))
    local x=E+INNER*.228+T.spacing.sm
    if busy then
        add(button("stop",x,AT,BTN+T.spacing.md,BTN,"","stop",get("chat.can_stop") and ui.action("chat.do","stop") or nil,false,BODY,ui.tr("bead.panel.chat-stop")))
        x=x+BTN+T.spacing.md+T.spacing.sm
    end
    add(button("latest",x,AT,INNER*.265,BTN,ui.tr("bead.panel.chat-latest"),"arrow_downward",ui.action("chat.do","latest"),false,CAPTION+1))
elseif section=="chats" then
    add(ui.Text{id="recent",position="absolute",left=E,top=TOP+T.spacing.xs+2,width=INNER*.7,height=24,text=ui.tr("bead.panel.chat-recent"),font_size=BODY,color="primary"})
    add(button("refresh",E+INNER*.73,TOP,INNER*.27,BTN,ui.tr("bead.panel.chat-refresh"),"refresh",ui.action("chat.do","refresh"),false,CAPTION+1))
    local LT=TOP+BTN+T.spacing.md+T.spacing.xs+2
    local note=get("chat.saved_state")
    if note~="" then
        add(ui.Text{id="note",position="absolute",left=E,top=LT,width=INNER,height=3*T.spacing.xxl,text=note,font_size=T.text.title*4/3-4,color="onSurfaceVariant"})
    end
    local RH=BTN+T.spacing.xxl-5
    add(ui.Scroll{id="saved",scroll=ui.bind("chat.qscroll"),position="absolute",left=E,top=LT,width=INNER,height=H-LT-E,
        ui.Column{id="saved-rows",width=INNER,gap=T.spacing.xs+2,
            ui.Repeat{id="rows",model=ui.bind("chat.saved"),key="id",render=function(item)
                return kit.Button{id="row",width=INNER,height=RH,padding=T.spacing.sm+1,radius=T.spacing.md,fill="surfaceContainer",aria_label=item.t,
                    on_activate=ui.action("chat.do",item.a),
                    ui.Column{id="saved-text",width=INNER-2*T.spacing.sm-2,height=RH-2*T.spacing.sm-2,gap=0,
                        ui.Text{id="saved-title",text=item.t,width=INNER-2*T.spacing.sm-2,height=22,font_size=BODY-1,color="onSurface"},
                        ui.Text{id="saved-detail",text=item.d,width=INNER-2*T.spacing.sm-2,height=16,font_size=CAPTION-1,color="onSurfaceVariant"}}}
            end}}})
else
    local provider=get("chat.provider")
    add(ui.Text{id="default",position="absolute",left=E,top=TOP+T.spacing.xs+2,width=INNER,height=24,text=ui.tr("bead.panel.chat-default-assistant"),font_size=BODY,color="primary"})
    add(button("codex",E,TOP+BTN+T.spacing.xs,INNER*.274,BTN,ui.tr("bead.panel.chat-codex"),"",ui.action("chat.do","provider:codex"),provider=="codex",CAPTION+1))
    add(button("claude",E+INNER*.274+T.spacing.sm,TOP+BTN+T.spacing.xs,INNER*.342,BTN,ui.tr("bead.panel.chat-claude"),"",ui.action("chat.do","provider:claude"),provider=="claude",CAPTION+1))
    add(ui.Text{id="hint",position="absolute",left=E,top=TOP+2*BTN+T.spacing.md,width=INNER,height=5*T.spacing.md,text=ui.tr("bead.panel.chat-default-hint"),font_size=T.text.title*4/3-4,color="onSurfaceVariant"})
    add(button("new",E,TOP+4*BTN+T.spacing.xs*3,INNER*.297,BTN,ui.tr("bead.panel.chat-new"),"add_comment",get("chat.can_new") and ui.action("chat.do","new") or nil,false,CAPTION+1))
end
return kit.Panel{id="chat",edge=get("view.edge"),open=ui.bind("panel.open"),duration=kit.motion.color,
    width=W,height=H,padding=0,radius=T.radii.medium+2,fill="surface",table.unpack(kids)}
