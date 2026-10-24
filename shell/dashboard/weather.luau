-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
local style=require("./weather-style")
local sky=require("./weather-sky")
return function()
    return ui.Box{id="weather",width=940,height=562,
        ui.Box{id="hero",position="absolute",left=0,top=0,width=style.width,height=style.height,radius=style.radius,fill="surfaceContainer",
            sky.background(),
            ui.Box{id="border",position="absolute",left=0,top=0,width=style.width,height=style.height,radius=style.radius,fill="transparent",border_width=1,border_color=ui.alpha(style.accent,style.border_alpha)},
            ui.Column{id="reading",position="absolute",left=style.padding,top=style.top,width=style.width*.55,height=240,gap=style.gap,
                ui.Text{id="location",text=ui.bind("dashboard.heading"),width=style.width*.55,height=22,font_size=style.location_size,color=style.muted},
                ui.Text{id="temperature",text=ui.bind("dashboard.metric"),font_size=style.temperature_size,height=82,color=style.ink},
                ui.Text{id="condition",text=ui.bind("dashboard.summary"),font_size=style.condition_size,height=30,width=style.width*.55,color=style.accent},
                ui.Text{id="details",text=ui.bind("dashboard.detail"),font_size=style.detail_size,height=36,width=style.width*.55,color=style.muted},
            },
        },
        ui.Column{id="hourly",position="absolute",left=0,top=310,width=564,height=252,padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            C.title("title",ui.tr("bead.dashboard.weather-hours"),532),
            ui.Row{id="hours",height=170,gap=8,
                ui.Repeat{id="items",model=ui.bind("dashboard.rows"),key="id",render=function(item)
                    return ui.Column{id="hour",width=59,height=160,gap=12,align="center",
                        ui.Text{text=item.value,height=30,font_size=20,color="primary"},
                        ui.Text{text=item.detail,width=59,height=48,font_size=12,text_align="center",color="onSurfaceVariant"},
                        ui.Text{text=item.label,height=24,font_size=14,color="onSurface"},
                    }
                end},
            },
        },
        ui.Column{id="outlook",position="absolute",left=580,top=0,width=360,height=562,padding=16,gap=16,radius=16,fill="surfaceContainerLow",
            C.title("title",ui.tr("bead.dashboard.weather-days"),328),
            C.rows("days","dashboard.secondary",328,470,false,ui.tr("bead.dashboard.weather-none")),
        },
    }
end
