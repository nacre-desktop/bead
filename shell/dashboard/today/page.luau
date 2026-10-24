-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local calendar=require("./summary-calendar")
local style=require("./style")

local function media(layout)
    local state=ui.resolve_for_check(ui.bind("dashboard.state"))
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local art=ui.resolve_for_check(ui.bind("dashboard.art"))
    local live=state=="playing" or state=="paused" or state=="stopped"
    local artSize=layout.artSize
    local function op(name,can)
        return control and ui.resolve_for_check(ui.bind(can)) and ui.action("dashboard.do",name) or nil
    end
    local function transport(id,glyph,label,action)
        return kit.Button{id=id,width=34,height=34,padding=0,radius=17,justify="center",align="center",
            fill="transparent",hover_fill=ui.alpha("onSurface",.08),aria_label=label,on_activate=action,
            ui.Text{text=glyph,family="Material Symbols Rounded",font_size=20,width=24,height=24,
                text_align="center",color=action and "onSurface" or "onSurfaceVariant"}}
    end
    local artwork=ui.Box{id="artwork",position="absolute",left=(layout.artContainer-artSize)/2,top=28,
        width=artSize,height=artSize,radius=artSize/2,fill="surfaceContainerHigh",
        art and ui.Scroll{id="album-crop",width=artSize,height=artSize,scroll=ui.bind("dashboard.hero.scroll"),
            ui.Image{icon="player-art",width=artSize,height=artSize,aria_label=ui.tr("bead.dashboard.media-art")}}
            or ui.Text{position="absolute",left=(artSize-40)/2,top=(artSize-40)/2,width=40,height=40,text="music_note",
                family="Material Symbols Rounded",font_size=layout.mediaArtIconSize,color="onSurfaceVariant"},
        ui.Image{position="absolute",left=0,top=0,width=artSize,height=artSize,path="dashboard/today/album-mask.themed.svg"}}
    local controls=ui.Row{id="buttons",position="absolute",left=(layout.media-114)/2,top=layout.mediaControlsY,
        width=114,height=34,gap=6,justify="center",
        transport("previous","skip_previous",ui.tr("bead.dashboard.media-previous"),op("player.previous","dashboard.can_previous")),
        transport("toggle",state=="playing" and "pause" or "play_arrow",
            state=="playing" and ui.tr("bead.dashboard.pause") or ui.tr("bead.dashboard.media-play"),op("player.play_pause","dashboard.can_toggle")),
        transport("next","skip_next",ui.tr("bead.dashboard.media-next"),op("player.next","dashboard.can_next"))}
    return ui.Box{id="media",position="absolute",left=layout.calendar+layout.host+32,top=0,
        width=layout.media,height=layout.summary,radius=layout.cardRadius,fill="surfaceContainerLow",
        ui.Box{id="album-container",position="absolute",left=(layout.media-layout.artContainer)/2,top=8,
            width=layout.artContainer,height=layout.artContainer,fill="transparent",artwork},
        ui.Text{id="track",position="absolute",left=14,top=layout.artContainer+16,width=layout.media-28,height=layout.mediaMetadataTitleHeight,
            text=live and ui.bind("dashboard.track") or ui.tr("bead.dashboard.playback-unavailable"),
            font_size=layout.mediaTitleSize,text_align="center",color="primary"},
        ui.Text{id="artist",position="absolute",left=14,top=layout.artContainer+16+layout.mediaMetadataTitleHeight+layout.mediaMetadataGap,width=layout.media-28,height=layout.mediaMetadataArtistHeight,
            text=live and ui.bind("dashboard.track_detail") or ui.tr("bead.dashboard.playback-unavailable-detail"),
            font_size=layout.mediaArtistSize,text_align="center",color="secondary"},
        controls,
    }
end

return function()
    local l=style.layout(ui.resolve_for_check(ui.bind("dashboard.hero.width")),
        ui.resolve_for_check(ui.bind("dashboard.hero.height"))+288+18)
    local calendarReady=ui.resolve_for_check(ui.bind("calendar.ready"))
    local host=ui.resolve_for_check(ui.bind("dashboard.host"))
    local gauges=ui.resolve_for_check(ui.bind("dashboard.gauges"))
    local wallpaper=ui.resolve_for_check(ui.bind("dashboard.wallpaper"))
    local hero=ui.Box{id="hero",width=l.width,height=l.hero,radius=l.heroRadius,fill="surfaceContainer",
        wallpaper and ui.Scroll{id="wallpaper-crop",width=l.width,height=l.hero,scroll=ui.bind("dashboard.hero.scroll"),
            ui.Row{id="wallpaper-centred",width=l.width,height=l.hero,shrink=0,justify="center",align="center",
                ui.Image{id="wallpaper",icon="today-wallpaper",width=l.width,height=l.hero,shrink=0}}}
            or ui.Box{id="wallpaper-empty",width=l.width,height=l.hero,fill="transparent"},
        ui.Image{id="shade",position="absolute",left=0,top=0,width=l.width,height=l.hero,path="dashboard/today/hero-shade.themed.svg"},
        ui.Column{id="clock-card",position="absolute",left=l.heroInset,top=l.heroInset,width=l.width-2*l.heroInset,
            height=l.heroTimeHeight+2*l.heroLineHeight+2*l.heroTextGap,gap=l.heroTextGap,
            ui.Text{id="time",text=ui.bind("dashboard.clock"),font_size=l.heroTimeSize,font_weight=300,height=l.heroTimeHeight,color=l.heroTextColor},
            ui.Text{id="date",text=ui.bind("dashboard.date"),font_size=l.heroDateSize,height=l.heroLineHeight,color=l.heroTextColor},
            ui.Text{id="weather",text=ui.bind("dashboard.summary"),font_size=l.heroWeatherSize,width=l.width-2*l.heroInset,height=l.heroLineHeight,color=l.heroTextColor},
        },
        ui.Row{id="palette",position="absolute",left=l.heroInset,top=l.hero-37,width=180,height=19,gap=10,
            ui.Box{width=19,height=19,radius=10,fill="primary"},ui.Box{width=19,height=19,radius=10,fill="secondary"},
            ui.Box{width=19,height=19,radius=10,fill="tertiary"},ui.Box{width=19,height=19,radius=10,fill="surfaceContainerHigh"},
            ui.Box{width=19,height=19,radius=10,fill="onSurface"}},
    }
    local hostRows={}
    local hostRowHeight=#host+1>5 and l.hostRowHeight or l.hostSmallRowHeight
    local hostRowGap=#host+1>5 and l.hostRowGap or l.hostSmallRowGap
    for i,item in ipairs(host) do
        hostRows[i]=ui.Row{id="host-"..item.id,width=l.hostInfo,height=hostRowHeight,gap=6,align="center",
            ui.Text{text=item.icon,family="Material Symbols Rounded",font_size=l.hostIconSize,width=20,height=20,
                color=i%2==0 and "onSurfaceVariant" or "primary"},
            ui.Text{text=item.text,width=l.hostInfo-26,height=20,font_size=l.hostTextSize,color="onSurface"}}
    end
    table.insert(hostRows,ui.Row{id="updates",width=l.hostInfo,height=hostRowHeight,gap=6,align="center",
        ui.Text{text="system_update",family="Material Symbols Rounded",font_size=l.hostIconSize,width=20,height=20,color="primary"},
        ui.Text{text=ui.bind("dashboard.updates"),width=l.hostInfo-26,height=20,font_size=l.hostTextSize,color="onSurface"}})
    local hostRowsHeight=#hostRows*hostRowHeight+(#hostRows-1)*hostRowGap
    local hostCard=ui.Row{id="host",width=l.host,height=l.hostBody,padding_left=18,padding_right=14,gap=16,
        radius=l.cardRadius,fill="surfaceContainerLow",align="center",
        ui.Row{id="distro-mark",width=l.logoSize,height=l.logoSize,radius=l.logoSize/2,fill="surfaceContainer",
            align="center",justify="center",ui.Image{path="dashboard/today/assets/nacre-logo.themed.svg",
                width=l.logoSize*l.logoContent,height=l.logoSize*l.logoContent,shrink=0,aria_label="Nacre"}},
        ui.Column{id="host-rows",width=l.hostInfo,height=hostRowsHeight,gap=hostRowGap,table.unpack(hostRows)},
    }
    local gaugeRow={}
    for i,metric in ipairs(gauges) do
        gaugeRow[i]=ui.Column{id=metric.id,width=(l.host-24)/3,height=l.metrics,padding=6,gap=1,
            ui.Row{id="gauge-heading",width=(l.host-36)/3,height=20,gap=4,align="center",
                ui.Text{text=metric.icon,family="Material Symbols Rounded",font_size=14,width=18,height=18,color="primary"},
                ui.Text{text=metric.label,width=(l.host-60)/3,height=18,font_size=10,color="onSurfaceVariant"}},
            ui.Text{text=metric.value,width=(l.host-36)/3,height=30,font_size=21,color="onSurface"},
        }
    end
    local summary=ui.Box{id="summary",position="absolute",left=0,top=l.hero+l.gap,width=l.width,height=l.summary,
        ui.Box{id="month",position="absolute",left=0,top=0,width=l.calendar,height=l.summary,
            radius=l.cardRadius,fill="surfaceContainerLow",calendarReady and calendar(l.calendar,l.summary) or ui.Text{id="unavailable",
                text=ui.tr("bead.panel.calendar-unavailable"),width=l.calendar-24,height=40,font_size=14,color="onSurfaceVariant"}},
        ui.Column{id="host-and-gauges",position="absolute",left=l.calendar+l.summaryGap,top=0,width=l.host,height=l.summary,gap=l.hostGap,
            hostCard,ui.Row{id="gauges",width=l.host,height=l.metrics,gap=0,radius=14,fill="surfaceContainerLow",table.unpack(gaugeRow)}},
        media(l),
    }
    return ui.Box{id="today",width=l.width,height=l.height,hero,summary}
end
