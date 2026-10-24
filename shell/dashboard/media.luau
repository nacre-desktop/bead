-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local C=require("./components")
local function transportButton(id,glyph,label,action)
    return kit.Button{id=id,width=34,height=34,padding=0,radius=17,justify="center",align="center",
        fill="transparent",hover_fill=ui.alpha("onSurface",.08),aria_label=label,on_activate=action,
        ui.Text{text=glyph,family="Material Symbols Rounded",font_size=20,width=24,height=24,
            text_align="center",color=action and "onSurface" or "onSurfaceVariant"}}
end
return function()
    local rows=ui.resolve_for_check(ui.bind("dashboard.rows"))
    local lyrics
    if #rows==0 then
        lyrics=ui.Text{id="plain",text=ui.bind("dashboard.detail"),width=328,height=400,font_size=16,color="onSurface"}
    else
        lyrics=ui.Column{id="lines",width=328,height=400,gap=12,
            ui.Repeat{id="items",model=ui.bind("dashboard.rows"),key="id",render=function(item)
                return ui.Text{id="line",text=item.label,width=328,height=60,font_size=16,color="onSurface"}
            end},
        }
    end
    local state=ui.resolve_for_check(ui.bind("dashboard.state"))
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local progress=ui.resolve_for_check(ui.bind("dashboard.progress"))
    local art=ui.resolve_for_check(ui.bind("dashboard.art"))
    local live=state=="playing" or state=="paused" or state=="stopped"
    local transport
    if live then
        local function op(name,can) return control and ui.resolve_for_check(ui.bind(can)) and ui.action("dashboard.do",name) or nil end
        transport=ui.Column{id="transport",width=520,height=146,gap=10,
            ui.Row{id="buttons",width=520,height=40,gap=8,
                transportButton("previous","skip_previous",ui.tr("bead.dashboard.media-previous"),op("player.previous","dashboard.can_previous")),
                transportButton("toggle",state=="playing" and "pause" or "play_arrow",
                    state=="playing" and ui.tr("bead.dashboard.pause") or ui.tr("bead.dashboard.media-play"),op("player.play_pause","dashboard.can_toggle")),
                transportButton("next","skip_next",ui.tr("bead.dashboard.media-next"),op("player.next","dashboard.can_next")),
                ui.Text{id="position",text=ui.bind("dashboard.metric"),width=170,height=24,font_size=14,text_align="right",color="onSurfaceVariant"},
            },
            ui.Box{id="track",width=520,height=6,radius=3,fill=ui.alpha("primary",0.16),
                progress>0 and ui.Box{id="elapsed",width=math.max(1,math.floor(520*progress)),height=6,radius=3,fill="primary"} or nil},
            ui.Row{id="seek",width=520,height=40,gap=12,
                C.button("seek-back",ui.tr("bead.dashboard.media-seek-back"),op("player.seek:-10","dashboard.can_seek"),110),
                C.button("seek-forward",ui.tr("bead.dashboard.media-seek-forward"),op("player.seek:10","dashboard.can_seek"),110),
            },
        }
    end
    return ui.Box{id="media",width=940,height=562,
        ui.Column{id="player",position="absolute",left=0,top=0,width=564,height=562,padding=22,gap=20,radius=20,fill="surfaceContainer",
            ui.Box{id="artwork",width=220,height=220,radius=20,fill="surfaceContainerHigh",
                art and ui.Image{position="absolute",left=0,top=0,icon="player-art",width=220,height=220,aria_label=ui.tr("bead.dashboard.media-art")}
                    or ui.Text{position="absolute",left=70,top=66,width=80,height=88,text="music_note",family="Material Symbols Rounded",font_size=80,color="primary"},
            },
            C.title("title",ui.bind("dashboard.track"),520),
            ui.Text{id="description",width=520,height=72,font_size=16,color="onSurfaceVariant",text=ui.bind("dashboard.track_detail")},
            transport,
        },
        ui.Column{id="lyrics",position="absolute",left=580,top=0,width=360,height=562,padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            C.title("title",ui.bind("dashboard.heading"),328),
            ui.Text{id="message",text=ui.bind("dashboard.summary"),width=328,height=48,font_size=14,color="onSurfaceVariant"},
            lyrics,
        },
    }
end
