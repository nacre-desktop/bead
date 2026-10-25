-- SPDX-License-Identifier: GPL-3.0-only
-- AI sidebar (B0043) composed from kit components. Geometry (edge, button height,
-- tab and content tops, footer positions) is projected from the size scale by the
-- adapter, so this file holds no pixel literals tied to one setup.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local px=function(pt) return pt*4/3 end
local BODY,CAPTION=px(T.text.body),px(T.text.caption)
local H=get("chat.height")
local section=get("chat.section")
local E,BTN,INNER,TABS,TOP=get("chat.edge"),get("chat.button"),get("chat.inner"),get("chat.tabs"),get("chat.top")
local W=INNER+2*E
local ICON=px(T.icons.medium)
local labels={}
for _,item in get("chat.labels") do labels[item.text]=item end
local kids={}
local function add(node) table.insert(kids,node) end
-- Intrinsic QML ActionButton proportions, shared through the kit.
local function button(id,x,y,w,h,label,icon,action,selected,size,aria)
    return kit.PillButton{id=id,position=x and "absolute" or nil,left=x,top=y,width=w,height=h,
        label=label,label_width=labels[label] and (size==CAPTION and labels[label].caption or labels[label].body) or nil,glyph=icon,on_activate=action,selected=selected,compact=size==CAPTION,aria_label=aria or label}
end
add(ui.Image{id="logo",position="absolute",left=E,top=E+T.spacing.hairline,width=T.icons.hero-T.spacing.hairline,height=T.icons.hero-T.spacing.hairline,
    path="chat/assets/nacre-ai.themed.svg",aria_label=ui.tr("bead.panel.chat-title")})
add(ui.Text{id="title",position="absolute",left=E+T.icons.hero+T.spacing.sm,top=E+T.spacing.xs-1,width=W/2,height=T.icons.hero,
    text=ui.tr("bead.panel.chat-title"),font_size=T.text.headline*4/3-T.spacing.xs/3,color="primary"})
local PIN_W=T.spacing.huge+ICON+T.spacing.xs+T.spacing.hairline/2
if not get("view.detached") then
    add(button("pin",W-E-PIN_W,E,PIN_W,BTN,"","push_pin",ui.action("chat.do","pin"),get("chat.pinned"),BODY,ui.tr("bead.panel.chat-pin")))
end
local tabs={}
for _,item in {{"chat",ui.tr("bead.panel.chat-tab-chat"),"forum"},{"chats",ui.tr("bead.panel.chat-tab-chats"),"history"},
    {"brain",ui.tr("bead.panel.chat-tab-brain"),"neurology"},{"settings","","settings"}} do
    table.insert(tabs,button("tab-"..item[1],nil,nil,nil,BTN,item[2],item[3],item[1]~="brain" and ui.action("chat.do","section:"..item[1]) or nil,section==item[1],BODY,item[2]~="" and item[2] or ui.tr("bead.panel.chat-tab-settings")))
end
add(ui.Row{id="tabs",position="absolute",left=E,top=TABS,width=INNER,height=BTN,gap=T.spacing.xs+T.spacing.hairline,table.unpack(tabs)})

if section=="chat" then
    add(ui.Row{id="tools",position="absolute",left=E,top=TOP,width=INNER,height=BTN,gap=T.spacing.sm,
        button("new",nil,nil,nil,BTN,ui.tr("bead.panel.chat-new"),"add_comment",get("chat.can_new") and ui.action("chat.do","new") or nil,false,BODY),
        button("workspace",nil,nil,nil,BTN,ui.tr("bead.panel.chat-workspace"),"open_in_new",get("chat.can_workspace") and ui.action("chat.do","workspace") or nil,false,BODY)})
    add(ui.Text{id="heading",position="absolute",left=E,top=TOP+BTN+T.spacing.sm-1,width=INNER,height=CAPTION*1.5,text=ui.bind("chat.heading"),
        font_size=CAPTION,color="onSurfaceVariant"})
    local TT,TH=get("chat.transcript_top"),get("chat.transcript_height")
    add(kit.Transcript{id="transcript",position="absolute",left=E,top=TT,width=INNER,height=TH,scroll=ui.bind("chat.scroll"),
        cells=ui.bind("chat.cells"),extent=get("chat.extent"),action_radius=T.radii.full,action_release=true,on_action="chat.do"})
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
        font_size=BODY,focus_ring=false,text=ui.bind("chat.draft"),placeholder=get("chat.placeholder"),aria_label=get("chat.placeholder"),
        on_activate=ui.action("chat.do","send")})
    local AT=get("chat.actions_top")
    local busy=get("chat.busy")
    local actions={button("send",nil,nil,nil,BTN,ui.tr("bead.panel.chat-send"),"arrow_upward",get("chat.can_send") and ui.action("chat.do","send") or nil,true,CAPTION)}
    -- The published ai.chat contract does not expose file/screenshot attachment actions yet.
    table.insert(actions,button("attach",nil,nil,nil,BTN,ui.tr("bead.panel.chat-attach"),"attach_file",nil,false,CAPTION))
    table.insert(actions,button("screenshot",nil,nil,nil,BTN,busy and "" or ui.tr("bead.panel.chat-screenshot"),"screenshot",nil,false,CAPTION))
    if busy then table.insert(actions,button("stop",nil,nil,nil,BTN,"","stop",get("chat.can_stop") and ui.action("chat.do","stop") or nil,false,CAPTION,ui.tr("bead.panel.chat-stop"))) end
    table.insert(actions,button("latest",nil,nil,nil,BTN,ui.tr("bead.panel.chat-latest"),"arrow_downward",ui.action("chat.do","latest"),false,CAPTION))
    add(ui.Row{id="actions",position="absolute",left=E,top=AT,width=INNER,height=BTN,gap=T.spacing.xs+T.spacing.hairline,table.unpack(actions)})
elseif section=="chats" then
    add(ui.Text{id="recent",position="absolute",left=E,top=TOP+T.spacing.xs+2,width=INNER*.7,height=24,text=ui.tr("bead.panel.chat-recent"),font_size=BODY,color="primary"})
    add(button("refresh",E+INNER*.73,TOP,INNER*.27,BTN,ui.tr("bead.panel.chat-refresh"),"refresh",ui.action("chat.do","refresh"),false,CAPTION+1))
    local LT=TOP+BTN+T.spacing.md+T.spacing.xs+2
    local note=get("chat.saved_state")
    if note~="" then
        add(ui.Text{id="note",position="absolute",left=E,top=LT,width=INNER,height=3*T.spacing.xxl,text=note,font_size=T.text.title*4/3-4,color="onSurfaceVariant"})
    end
    local RH=BTN+T.spacing.xxl-5
    add(require("../kit/virtual-list"){id="saved",scroll=ui.bind("chat.qscroll"),position="absolute",left=E,top=LT,
        width=INNER,height=H-LT-E,model=ui.bind("chat.saved"),key="id",row_height=RH,gap=T.spacing.xs+2,repeat_id="rows",
        render=function(item)
            return kit.Button{id="row",width=INNER,height=RH,padding=T.spacing.sm+1,radius=T.spacing.md,fill="surfaceContainer",aria_label=item.t,
                on_activate=ui.action("chat.do",item.a),
                ui.Column{id="saved-text",width=INNER-2*T.spacing.sm-2,height=RH-2*T.spacing.sm-2,gap=0,
                    ui.Text{id="saved-title",text=item.t,width=INNER-2*T.spacing.sm-2,height=22,font_size=BODY-1,color="onSurface"},
                    ui.Text{id="saved-detail",text=item.d,width=INNER-2*T.spacing.sm-2,height=16,font_size=CAPTION-1,color="onSurfaceVariant"}}}
        end})
else
    local provider=get("chat.provider")
    add(ui.Column{id="preferences",position="absolute",left=E,top=TOP,width=INNER,gap=T.spacing.md,
        ui.Text{id="default",text=ui.tr("bead.panel.chat-default-assistant"),width=INNER,height=BODY*1.4,font_size=BODY,color="primary"},
        ui.Row{id="providers",width=INNER,height=BTN,gap=T.spacing.sm,
            button("codex",nil,nil,nil,BTN,ui.tr("bead.panel.chat-codex"),"",ui.action("chat.do","provider:codex"),provider=="codex",BODY),
            button("claude",nil,nil,nil,BTN,ui.tr("bead.panel.chat-claude"),"",ui.action("chat.do","provider:claude"),provider=="claude",BODY)},
        ui.Text{id="hint",text=ui.tr("bead.panel.chat-default-hint"),width=INNER,height=3*T.spacing.md+T.spacing.xs,font_size=CAPTION+T.spacing.xs/3,color="onSurfaceVariant"},
        button("new",nil,nil,nil,BTN,ui.tr("bead.panel.chat-new"),"add_comment",get("chat.can_new") and ui.action("chat.do","new") or nil,false,BODY)})
end
return kit.Panel{id="chat",edge=get("view.edge"),open=ui.bind("panel.open"),duration=kit.motion.text,return_duration=.18,return_transition="ease",transition="ease-in-out",fade=false,reveal=not ui.resolve_for_check(ui.bind("view.detached")),
    width=W,height=H,padding=0,radius=T.radii.medium+2,fill="surface",table.unpack(kids)}
