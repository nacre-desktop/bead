-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
local calendar=require("./calendar")
return function()
    local calendarReady=ui.resolve_for_check(ui.bind("calendar.ready"))
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
        },
        ui.Box{id="month",position="absolute",left=0,top=244,width=310,height=318,padding=15,radius=16,fill="surfaceContainerLow",calendarReady and calendar() or ui.Text{id="unavailable",text="Calendar unavailable",width=280,height=40,font_size=16,color="onSurfaceVariant"}},
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
