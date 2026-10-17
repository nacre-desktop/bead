-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
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
    return ui.Box{id="media",width=940,height=562,
        ui.Column{id="player",position="absolute",left=0,top=0,width=564,height=562,padding=22,gap=20,radius=20,fill="surfaceContainer",
            ui.Box{id="artwork",width=220,height=220,radius=20,fill="surfaceContainerHigh",
                ui.Text{position="absolute",left=70,top=66,width=80,height=88,text="music_note",family="Material Symbols Rounded",font_size=80,color="primary"},
            },
            C.title("title","Playback unavailable",520),
            ui.Text{id="description",width=520,height=72,font_size=16,color="onSurfaceVariant",text="Playback details and controls are not available yet"},
        },
        ui.Column{id="lyrics",position="absolute",left=580,top=0,width=360,height=562,padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            C.title("title",ui.bind("dashboard.heading"),328),
            ui.Text{id="message",text=ui.bind("dashboard.summary"),width=328,height=48,font_size=14,color="onSurfaceVariant"},
            lyrics,
        },
    }
end
