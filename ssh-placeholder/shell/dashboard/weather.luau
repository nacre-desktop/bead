-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
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
    local weekdays={"bead.common.weekday-short-sun","bead.common.weekday-short-mon","bead.common.weekday-short-tue","bead.common.weekday-short-wed","bead.common.weekday-short-thu","bead.common.weekday-short-fri","bead.common.weekday-short-sat"}
    local function weekday(date)
        local y,m,d=string.match(date,"^(%d%d%d%d)%-(%d%d)%-(%d%d)$")
        if not y then return date end
        y,m,d=tonumber(y),tonumber(m),tonumber(d)
        local offsets={0,3,2,5,0,3,5,1,4,6,2,4}
        if m<1 or m>12 then return date end
        if m<3 then y-=1 end
        local day=(y+math.floor(y/4)-math.floor(y/100)+math.floor(y/400)+offsets[m]+d)%7
        return ui.tr(weekdays[day+1])
    end
    local function condition(code)
        if type(code)~="number" or code<0 then return "cloud_off" end
        return code<=1 and "sunny" or code==2 and "partly_cloudy_day" or code==3 and "cloud"
            or (code==45 or code==48) and "foggy" or code>=95 and "thunderstorm"
            or (code>=71 and code<=77 or code==85 or code==86) and "weather_snowy" or "rainy"
    end
    local hours={}
    local low,high=math.huge,-math.huge
    for _,item in ipairs(ui.resolve_for_check(ui.bind("dashboard.rows"))) do
        if type(item.temperature)=="number" then
            table.insert(hours,item)
            low,high=math.min(low,item.temperature),math.max(high,item.temperature)
        end
    end
    local chartWidth,chartHeight=style.width-32,160
    local points,area,labels={},{},{}
    local slots=0
    for i in ipairs(hours) do if (i-1)%4==0 or i==#hours then slots+=1 end end
    local cell=1/math.max(1,slots)
    local slotWidth=math.max(28,chartWidth*cell*.75)
    for i,item in ipairs(hours) do
        local x=cell/2+(i-1)/math.max(1,#hours-1)*(1-cell)
        local y=(16+(1-(item.temperature-low)/math.max(1,high-low))*(chartHeight-24))/chartHeight
        table.insert(points,{x,y})
        if (i-1)%4==0 or i==#hours then
            table.insert(labels,ui.Text{id="temperature-"..item.id,position="absolute",left=16+x*chartWidth-34,
                top=42+y*chartHeight-22,width=68,height=20,text=item.value,font_size=12,text_align="center",color="onSurface"})
            table.insert(labels,ui.Column{id="time-"..item.id,position="absolute",left=16+x*chartWidth-slotWidth/2,
                top=210,width=slotWidth,height=40,gap=4,align="center",
                ui.Row{width=slotWidth,height=18,gap=4,justify="center",
                    ui.Text{text=condition(item.code),width=20,height=18,font_size=16,family="Material Symbols Rounded",color="secondary"},
                    ui.Text{text=item.rain_text or item.detail,height=18,font_size=10.667,color="onSurface"}},
                ui.Text{text=item.label,width=slotWidth,height=16,font_size=10.667,text_align="center",color="onSurfaceVariant"}})
        end
    end
    if #points>=2 then
        table.insert(area,{points[1][1],1})
        for _,point in ipairs(points) do table.insert(area,point) end
        table.insert(area,{points[#points][1],1})
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
        ui.Box{id="hourly",position="absolute",left=0,top=310,width=style.width,height=252,radius=16,fill="surfaceContainer",
            ui.Text{id="title",position="absolute",left=16,top=12,width=220,height=24,
                text=ui.tr("bead.dashboard.weather-hours"),font_size=16,font_weight=600,color="onSurface"},
            ui.Text{id="legend",position="absolute",right=16,top=16,width=250,height=18,
                text=ui.tr("bead.dashboard.weather-hour-legend"),font_size=10.667,text_align="right",color="onSurfaceVariant"},
            ui.Polyline{id="area",position="absolute",left=16,top=42,width=chartWidth,height=chartHeight,
                points=area,closed=true,stroke_width=2,color={0,0,0,0},fill=ui.alpha("primary",.08)},
            ui.Polyline{id="temperature",position="absolute",left=16,top=42,width=chartWidth,height=chartHeight,
                points=points,stroke_width=2,color="primary",round_caps=true},
            ui.Text{id="empty",visible=#hours==0,position="absolute",left=16,top=100,width=chartWidth,height=24,
                text=ui.tr("bead.dashboard.weather-none"),font_size=13.333,text_align="center",color="onSurfaceVariant"},
            table.unpack(labels),
        },
        ui.Box{id="outlook",position="absolute",left=580,top=0,width=360,height=562,radius=16,fill="surfaceContainer",
            ui.Text{id="title",position="absolute",left=16,top=12,width=328,height=24,
                text=ui.tr("bead.dashboard.weather-days"),font_size=16,font_weight=600,color="onSurface"},
            ui.Column{id="days",position="absolute",left=16,top=54,width=328,height=496,gap=0,
                ui.Repeat{id="items",model=ui.bind("dashboard.secondary"),key="id",render=function(item)
                    local valid=type(item.low)=="number" and type(item.high)=="number"
                    local rowHeight=496/math.max(1,#days)
                    local textY=(rowHeight-20)/2
                    local barY=(rowHeight-5)/2
                    local rangeStart=valid and math.clamp((item.low-minimum)/span,0,1) or 0
                    local rangeWidth=valid and math.max(3,328*.24*math.clamp((item.high-item.low)/span,0,1)) or 0
                    return ui.Box{id="day",width=328,height=rowHeight,
                        ui.Text{position="absolute",left=0,top=textY,text=weekday(item.label),width=40,height=20,font_size=12,color="onSurface"},
                        ui.Text{position="absolute",left=328*.21,top=textY,text=condition(item.code),width=24,height=20,font_size=14.667,
                            family="Material Symbols Rounded",color="secondary"},
                        ui.Text{position="absolute",left=328*.32,top=textY,text=item.low_text or item.value,width=44,height=20,font_size=12,color="onSurfaceVariant"},
                        ui.Box{position="absolute",left=328*.47,top=barY,width=328*.24,height=5,radius=3,fill=ui.alpha("primary",.12)},
                        ui.Box{position="absolute",visible=valid,left=328*.47+328*.24*rangeStart,top=barY,
                            width=rangeWidth,height=5,radius=3,fill="primary"},
                        ui.Text{position="absolute",left=328*.75,top=textY,text=item.high_text or "",width=46,height=20,font_size=12,color="onSurface"},
                        ui.Text{position="absolute",right=0,top=textY,text=item.rain_text or item.detail,width=54,height=20,font_size=12,text_align="right",color="onSurfaceVariant"}}
                end},
            },
        },
    }
end
