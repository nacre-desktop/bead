-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local function iconButton(id,glyph,label,action,size,primary)
    size=size or 42
    return kit.Button{id=id,width=size,height=size,padding=0,radius=size/2,justify="center",align="center",
        fill=primary and ui.alpha("primary",.18) or "transparent",
        hover_fill=ui.alpha("onSurface",.08),aria_label=label,on_activate=action,
        ui.Text{text=glyph,family="Material Symbols Rounded",font_size=primary and 28 or 22,
            width=size-8,height=size-8,text_align="center",color=action and "onSurface" or "onSurfaceVariant"}}
end
local function seekButton(id,glyph,label,action)
    return iconButton(id,glyph,label,action,36,false)
end
return function()
    local state=ui.resolve_for_check(ui.bind("dashboard.state"))
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local art=ui.resolve_for_check(ui.bind("dashboard.art"))
    local progress=ui.resolve_for_check(ui.bind("dashboard.progress"))
    local can=function(key) return control and ui.resolve_for_check(ui.bind(key)) end
    local op=function(name,key) return can(key) and ui.action("dashboard.do",name) or nil end
    local rows=ui.resolve_for_check(ui.bind("dashboard.rows"))
    local lyricBody
    if #rows>0 then
        local lines={}
        for i,item in ipairs(rows) do
            lines[i]=ui.Text{id="line-"..item.id,text=item.label,width=303,height=60,
                font_size=17*4/3,color="onSurfaceVariant"}
        end
        lyricBody=ui.Column{id="lines",width=303,height=348,gap=6,table.unpack(lines)}
    else
        lyricBody=ui.Text{id="plain",text=ui.bind("dashboard.detail"),width=303,height=348,
            font_size=15*4/3,color="onSurfaceVariant"}
    end

    local identity=ui.Row{id="identity",position="absolute",left=22,top=16,width=190,height=38,
        padding_left=12,padding_right=12,gap=8,align="center",radius=19,fill=ui.alpha("primary",.12),
        ui.Text{text="music_note",family="Material Symbols Rounded",font_size=18,width=22,height=22,color="primary"},
        ui.Text{text=ui.bind("dashboard.title"),width=140,height=22,font_size=12*4/3,color="onSurface"}}

    local coverSize=280
    local metadata=ui.Column{id="metadata",position="absolute",left=coverSize+22,top=0,width=213,height=310,
        gap=12,justify="center",
        ui.Text{id="title",text=ui.bind("dashboard.track"),width=213,height=90,font_size=23*4/3,
            font_weight=600,color="onSurface"},
        ui.Text{id="description",text=ui.bind("dashboard.track_detail"),width=213,height=52,
            font_size=13*4/3,color="secondary"},
    }
    local artwork=ui.Box{id="artwork",position="absolute",left=0,top=15,width=coverSize,height=coverSize,
        radius=14,fill="surfaceContainerHigh",
        art and ui.Image{position="absolute",left=0,top=0,icon="player-art",width=coverSize,height=coverSize,
            aria_label=ui.tr("bead.dashboard.media-art")}
            or ui.Text{position="absolute",left=98,top=98,width=84,height=84,text="music_note",
                family="Material Symbols Rounded",font_size=82,color="primary"}}
    local seeking=ui.Row{id="seek",position="absolute",left=0,top=318,width=539,height=40,
        gap=8,align="center",
        seekButton("seek-back","replay_10",ui.tr("bead.dashboard.media-seek-back"),op("player.seek:-10","dashboard.can_seek")),
        ui.Column{id="progress",width=451,height=34,gap=4,
            ui.Text{id="position",text=ui.bind("dashboard.metric"),width=451,height=18,font_size=11*4/3,
                text_align="center",color="onSurfaceVariant"},
            ui.Box{id="track",width=451,height=5,radius=3,fill=ui.alpha("primary",.16),
                progress>0 and ui.Box{id="elapsed",width=math.floor(451*progress),height=5,radius=3,fill="primary"} or nil}},
        seekButton("seek-forward","forward_10",ui.tr("bead.dashboard.media-seek-forward"),op("player.seek:10","dashboard.can_seek")),
    }
    local transport=ui.Row{id="transport",position="absolute",left=0,top=372,width=539,height=56,
        gap=18,justify="center",align="center",
        iconButton("previous","skip_previous",ui.tr("bead.dashboard.media-previous"),op("player.previous","dashboard.can_previous"),44),
        iconButton("toggle",state=="playing" and "pause" or "play_arrow",
            state=="playing" and ui.tr("bead.dashboard.pause") or ui.tr("bead.dashboard.media-play"),
            op("player.play_pause","dashboard.can_toggle"),56,true),
        iconButton("next","skip_next",ui.tr("bead.dashboard.media-next"),op("player.next","dashboard.can_next"),44),
    }
    return ui.Box{id="media",width=940,height=562,radius=20,fill="surfaceContainer",
        identity,
        ui.Box{id="player",position="absolute",left=22,top=60,width=539,height=480,
            artwork,metadata,seeking,transport},
        ui.Column{id="lyrics",position="absolute",left=583,top=60,width=335,height=480,
            padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            ui.Text{id="heading",text=ui.bind("dashboard.heading"),width=303,height=28,font_size=10*4/3,
                color="onSurfaceVariant"},
            ui.Text{id="message",text=ui.bind("dashboard.summary"),width=303,height=48,font_size=12*4/3,
                color="onSurfaceVariant"},
            lyricBody,
        },
    }
end
