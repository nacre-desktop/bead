-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
local calendar=require("./calendar")
return function()
    local calendarReady=ui.resolve_for_check(ui.bind("calendar.ready"))
    local state=ui.resolve_for_check(ui.bind("dashboard.state"))
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local art=ui.resolve_for_check(ui.bind("dashboard.art"))
    local function op(name,can) return control and ui.resolve_for_check(ui.bind(can)) and ui.action("dashboard.do",name) or nil end
    -- Mini player: only while the daemon reports a player.
    local mini
    if state=="playing" or state=="paused" or state=="stopped" then
        mini=ui.Column{id="mini",position="absolute",left=616,top=24,width=300,height=180,padding=14,gap=8,radius=16,fill="surfaceContainerHigh",
            ui.Row{id="now",width=272,height=72,gap=12,
                ui.Box{id="artwork",width=72,height=72,radius=12,fill="surfaceContainer",
                    art and ui.Image{position="absolute",left=0,top=0,icon="player-art",width=72,height=72,aria_label=ui.tr("bead.dashboard.media-art")}
                        or ui.Text{position="absolute",left=16,top=16,width=40,height=40,text="music_note",family="Material Symbols Rounded",font_size=36,color="primary"}},
                ui.Column{width=188,height=72,gap=2,
                    ui.Text{id="track",text=ui.bind("dashboard.track"),width=188,height=28,font_size=18,color="onSurface"},
                    ui.Text{id="artist",text=ui.bind("dashboard.track_detail"),width=188,height=40,font_size=14,color="onSurfaceVariant"},
                },
            },
            ui.Row{id="buttons",width=272,height=40,gap=8,
                C.button("previous",ui.tr("bead.dashboard.media-prev"),op("player.previous","dashboard.can_previous"),84),
                C.button("toggle",state=="playing" and ui.tr("bead.dashboard.pause") or ui.tr("bead.dashboard.media-play"),op("player.play_pause","dashboard.can_toggle"),84),
                C.button("next",ui.tr("bead.dashboard.media-next"),op("player.next","dashboard.can_next"),84),
            },
        }
    end
    return ui.Box{id="today",width=940,height=562,
        ui.Column{id="hero",position="absolute",left=0,top=0,width=940,height=228,padding=24,gap=10,radius=20,fill="surfaceContainer",
            ui.Text{id="time",text=ui.bind("dashboard.clock"),font_size=64,height=84,color="onSurface"},
            ui.Text{id="date",text=ui.bind("dashboard.date"),font_size=18,height=28,color="onSurfaceVariant"},
            ui.Text{id="weather",text=ui.bind("dashboard.summary"),font_size=18,width=892,height=28,color="onSurface"},
            ui.Row{id="palette",height=20,gap=10,
                ui.Box{width=19,height=19,radius=10,fill="primary"},
                ui.Box{width=19,height=19,radius=10,fill="secondary"},
                ui.Box{width=19,height=19,radius=10,fill="tertiary"},
                ui.Box{width=19,height=19,radius=10,fill="surfaceContainerHigh"},
                ui.Box{width=19,height=19,radius=10,fill="onSurface"},
            },
            mini,
        },
        ui.Box{id="month",position="absolute",left=0,top=244,width=310,height=318,padding=15,radius=16,fill="surfaceContainerLow",calendarReady and calendar() or ui.Text{id="unavailable",text=ui.tr("bead.panel.calendar-unavailable"),width=280,height=40,font_size=16,color="onSurfaceVariant"}},
        ui.Column{id="agenda",position="absolute",left=326,top=244,width=614,height=318,padding=16,gap=8,radius=16,fill="surfaceContainerLow",
            C.title("heading",ui.bind("dashboard.heading"),582),
            ui.Text{id="empty",text=ui.bind("dashboard.detail"),width=582,height=24,font_size=14,color="onSurfaceVariant"},
            ui.Scroll{id="events",width=582,height=224,scroll=ui.bind("dashboard.agenda.scroll.y"),
                ui.Column{width=582,shrink=0,gap=10,
                    ui.Repeat{id="items",model=ui.bind("dashboard.rows"),key="id",render=function(item)
                        return ui.Row{id="event",width=582,height=48,gap=12,align="center",
                            ui.Column{width=140,height=48,gap=2,
                                ui.Text{text=item.value,width=140,height=24,font_size=16,color="primary"},
                                ui.Text{text=item.detail,width=140,height=20,font_size=12,color="onSurfaceVariant"},
                            },
                            ui.Text{text=item.label,width=430,height=48,font_size=16,color="onSurface"},
                        }
                    end},
                },
            },
        },
    }
end
