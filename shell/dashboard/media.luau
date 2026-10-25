-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local function iconButton(id,glyph,label,action,size,primary)
    size=size or 42
    return kit.Button{id=id,width=size,height=size,padding=0,radius=size/2,justify="center",align="center",
        fill=primary and "primary" or "transparent",
        hover_fill=ui.alpha("onSurface",.08),aria_label=label,on_activate=action,
        ui.Text{text=glyph,family="Material Symbols Rounded",font_size=primary and 28 or 22,
            width=size-8,height=size-8,text_align="center",
            color=primary and "onPrimary" or (action and "onSurface" or "onSurfaceVariant")}}
end
return function()
    local state=ui.resolve_for_check(ui.bind("dashboard.state"))
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local art=ui.resolve_for_check(ui.bind("dashboard.art"))
    local progress=ui.resolve_for_check(ui.bind("dashboard.progress"))
    local can=function(key) return control and ui.resolve_for_check(ui.bind(key)) end
    local op=function(name,key) return can(key) and ui.action("dashboard.do",name) or nil end
    local rows=ui.resolve_for_check(ui.bind("dashboard.rows"))
    -- Timed lyric cues and shuffle/repeat controls await published player fields/actions.
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

    local player=ui.resolve_for_check(ui.bind("dashboard.player_identity"))
    local identityWidth=math.min(280,math.max(96,#player*8+24))
    local identity=ui.Row{id="identity",position="absolute",left=22,top=16,width=identityWidth,height=38,
        padding_left=12,padding_right=12,align="center",justify="center",radius=19,fill=ui.alpha("primary",.12),
        ui.Text{text=player,width=identityWidth-24,height=22,font_size=16,font_weight=600,text_align="center",color="onSurface"}}

    local coverSize=280.176
    local metadata=ui.Column{id="metadata",position="absolute",left=302.176,top=22,width=236.624,height=280.176,
        gap=12,justify="center",
        ui.Text{id="title",text=ui.bind("dashboard.track"),width=236.624,font_size=23*4/3,
            font_weight=600,color="onSurface"},
        ui.Text{id="description",text=ui.bind("dashboard.artist"),width=236.624,
            font_size=13*4/3,color="secondary"},
        ui.Text{id="album",text=ui.bind("dashboard.album"),width=236.624,height=20,
            font_size=10*4/3,color="onSurfaceVariant"},
    }
    local artwork=ui.Box{id="artwork",position="absolute",left=0,top=36.912,width=coverSize,height=coverSize,
        radius=14,fill="surfaceContainerHigh",
        art and ui.Image{position="absolute",left=0,top=0,icon="player-art",width=coverSize,height=coverSize,
            aria_label=ui.tr("bead.dashboard.media-art")}
            or ui.Text{position="absolute",left=98.088,top=98.088,width=84,height=84,text="music_note",
                family="Material Symbols Rounded",font_size=82,color="primary"}}
    local seekWidth=538.8
    local seeking=ui.Box{id="seek",position="absolute",left=0,top=348,width=seekWidth,height=26,
        ui.Box{id="track",position="absolute",left=0,top=10,width=seekWidth,height=6,radius=3,fill=ui.alpha("primary",.16),
            progress>0 and ui.Box{id="elapsed",width=math.floor(seekWidth*progress),height=6,radius=3,fill="primary"} or nil}}
    local seekDuration=ui.Box{id="seek-duration",position="absolute",left=0,top=378,width=seekWidth,height=16,
        kit.Button{id="seek-back",position="absolute",left=0,top=0,width=32,height=16,padding=0,radius=4,
            fill="transparent",hover_fill=ui.alpha("onSurface",.08),aria_label=ui.tr("bead.dashboard.media-seek-back"),
            on_activate=op("player.seek:-10","dashboard.can_seek"),
            ui.Text{text="replay_10",family="Material Symbols Rounded",font_size=12,width=28,height=16,
                text_align="center",color="onSurfaceVariant"}},
        ui.Text{id="position",position="absolute",left=34,top=0,width=470,height=16,
            text=ui.bind("dashboard.metric"),font_size=10*4/3,text_align="center",color="onSurfaceVariant"},
        kit.Button{id="seek-forward",position="absolute",right=0,top=0,width=32,height=16,padding=0,radius=4,
            fill="transparent",hover_fill=ui.alpha("onSurface",.08),aria_label=ui.tr("bead.dashboard.media-seek-forward"),
            on_activate=op("player.seek:10","dashboard.can_seek"),
            ui.Text{text="forward_10",family="Material Symbols Rounded",font_size=12,width=28,height=16,
                text_align="center",color="onSurfaceVariant"}}}
    local transport=ui.Row{id="transport",position="absolute",left=0,top=398,width=538.8,height=56,
        gap=12,justify="center",align="center",
        iconButton("previous","skip_previous",ui.tr("bead.dashboard.media-previous"),op("player.previous","dashboard.can_previous"),42),
        iconButton("toggle",state=="playing" and "pause" or "play_arrow",
            state=="playing" and ui.tr("bead.dashboard.pause") or ui.tr("bead.dashboard.media-play"),
            op("player.play_pause","dashboard.can_toggle"),56,true),
        iconButton("next","skip_next",ui.tr("bead.dashboard.media-next"),op("player.next","dashboard.can_next"),42),
    }
    local hasLyrics=#rows>0 or ui.resolve_for_check(ui.bind("dashboard.detail"))~=""
    local clock=ui.resolve_for_check(ui.bind("dashboard.metric"))
    local duration=string.match(clock,"/ (.+)$") or "—"
    local details=ui.Column{id="track-details",width=303,height=348,gap=20,
        ui.Text{text=ui.bind("dashboard.album"),width=303,font_size=24,font_weight=600,color="onSurface"},
        ui.Text{text=ui.bind("dashboard.artist"),width=303,font_size=12,color="onSurfaceVariant"},
        ui.Text{text=ui.tr("bead.dashboard.media-duration",{value=duration}),width=303,font_size=12,color="onSurfaceVariant"},
        ui.Text{text=player,width=303,font_size=12,color="onSurfaceVariant"}}
    return ui.Box{id="media",width=940,height=562,radius=20,fill="surfaceContainer",
        ui.Group{id="backdrop",visible=art,position="absolute",left=0,top=0,width=940,height=562,
            ui.Image{position="absolute",left=0,top=0,width=940,height=562,icon="player-art",blur=32},
            ui.Box{position="absolute",left=0,top=0,width=940,height=562,radius=20,fill=ui.alpha("surfaceContainer",.9)}},
        ui.Box{id="rim",position="absolute",left=0,top=0,width=940,height=562,radius=20,fill="transparent",
            border_width=1,border_color=ui.alpha("primary",.25)},
        identity,
        ui.Box{id="player",position="absolute",left=22,top=60,width=538.8,height=480,
            artwork,metadata,seeking,seekDuration,transport},
        ui.Column{id="lyrics",position="absolute",left=583,top=60,width=335,height=480,
            padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            ui.Text{id="heading",text=hasLyrics and ui.bind("dashboard.heading") or ui.tr("bead.dashboard.media-track-details"),width=303,height=28,font_size=10*4/3,
                color="onSurfaceVariant"},
            ui.Text{id="message",visible=hasLyrics,text=ui.bind("dashboard.summary"),width=303,height=48,font_size=12*4/3,
                color="onSurfaceVariant"},
            hasLyrics and lyricBody or details,
        },
    }
end
