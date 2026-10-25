-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
local style=require("./weather-style")
local sky=require("./weather-sky")
return function()
    local metrics={}
    for i,item in ipairs(ui.resolve_for_check(ui.bind("dashboard.weather.metrics"))) do
        table.insert(metrics,ui.Column{id=item.id,position="absolute",left=20+((i-1)%3)*(style.width-40)/3,
            top=style.height-118+math.floor((i-1)/3)*50,width=(style.width-40)/3,height=42,gap=3,
            ui.Text{text=item.label,width=(style.width-40)/3,height=16,font_size=12,color=style.muted},
            ui.Text{text=item.value,width=(style.width-40)/3,height=23,font_size=17.333,font_weight=600,color=style.ink}})
    end
    local days=ui.resolve_for_check(ui.bind("dashboard.secondary"))
    local minimum,maximum=math.huge,-math.huge
    for _,day in ipairs(days) do
        if type(day.low)=="number" then minimum=math.min(minimum,day.low) end
        if type(day.high)=="number" then maximum=math.max(maximum,day.high) end
    end
    local span=math.max(1,maximum-minimum)
    local weekdays={"sun","mon","tue","wed","thu","fri","sat"}
    local function weekday(date)
        local y,m,d=string.match(date,"^(%d%d%d%d)%-(%d%d)%-(%d%d)$")
        if not y then return date end
        y,m,d=tonumber(y),tonumber(m),tonumber(d)
        local offsets={0,3,2,5,0,3,5,1,4,6,2,4}
        if m<1 or m>12 then return date end
        if m<3 then y-=1 end
        local day=(y+math.floor(y/4)-math.floor(y/100)+math.floor(y/400)+offsets[m]+d)%7
        return ui.tr("bead.common.weekday-short-"..weekdays[day+1])
    end
    local function condition(code)
        if type(code)~="number" or code<0 then return "cloud_off" end
        return code<=1 and "sunny" or code==2 and "partly_cloudy_day" or code==3 and "cloud"
            or (code==45 or code==48) and "foggy" or code>=95 and "thunderstorm"
            or (code>=71 and code<=77 or code==85 or code==86) and "weather_snowy" or "rainy"
    end
    return ui.Box{id="weather",width=940,height=562,
        ui.Box{id="hero",position="absolute",left=0,top=0,width=style.width,height=style.height,radius=style.radius,fill="surfaceContainer",
            sky.background(),
            ui.Box{id="border",position="absolute",left=0,top=0,width=style.width,height=style.height,radius=style.radius,fill="transparent",border_width=1,border_color=ui.alpha(style.accent,style.border_alpha)},
            ui.Column{id="reading",position="absolute",left=style.padding,top=style.top,width=style.width*.55,height=160,gap=style.gap,
                ui.Text{id="location",text=ui.bind("dashboard.heading"),width=style.width*.55,height=22,font_size=style.location_size,color=style.muted},
                ui.Text{id="temperature",text=ui.bind("dashboard.metric"),font_size=style.temperature_size,height=82,color=style.ink},
                ui.Text{id="condition",text=ui.bind("dashboard.summary"),font_size=style.condition_size,height=30,width=style.width*.55,color=style.accent},
                ui.Text{id="details",text=ui.bind("dashboard.detail"),font_size=style.detail_size,height=36,width=style.width*.55,color=style.muted},
            },
            ui.Arc{id="sun-path",position="absolute",left=style.width*.6+8,top=45,
                width=style.width*.34-16,height=172,start_angle=180,sweep_angle=180,
                stroke_width=2,color=ui.alpha(style.accent,.45)},
            ui.Text{id="sun-times",position="absolute",left=style.width*.6,top=143,width=style.width*.34,height=28,
                text=ui.bind("dashboard.weather.sun_times"),font_size=10.667,text_align="center",color=style.muted},
            table.unpack(metrics),
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
            ui.Column{id="days",width=328,height=470,gap=8,
                ui.Repeat{id="items",model=ui.bind("dashboard.secondary"),key="id",render=function(item)
                    local valid=type(item.low)=="number" and type(item.high)=="number"
                    return ui.Box{id="day",width=328,height=60,
                        ui.Text{position="absolute",left=0,top=18,text=weekday(item.label),width=40,height=20,font_size=12,color="onSurface"},
                        ui.Text{position="absolute",left=48,top=18,text=condition(item.code),width=24,height=20,font_size=16,
                            family="Material Symbols Rounded",color="primary"},
                        ui.Text{position="absolute",left=80,top=18,text=item.low_text or item.value,width=44,height=20,font_size=12,color="onSurfaceVariant"},
                        ui.Box{position="absolute",left=128,top=25,width=78,height=5,radius=3,fill=ui.alpha("primary",.12)},
                        valid and ui.Box{position="absolute",left=128+78*(item.low-minimum)/span,top=25,
                            width=math.max(3,78*(item.high-item.low)/span),height=5,radius=3,fill="primary"} or nil,
                        ui.Text{position="absolute",left=216,top=18,text=item.high_text or "",width=46,height=20,font_size=12,color="onSurface"},
                        ui.Text{position="absolute",left=274,top=18,text=item.rain_text or item.detail,width=54,height=20,font_size=12,color="onSurfaceVariant"}}
                end},
            },
        },
    }
end
