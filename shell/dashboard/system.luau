-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
return function()
    -- One shared raised band, matching the QML resource gauges. Layout remains
    -- shell-owned; the host supplies only readings and fractions.
    local width,height=940,562
    local bandHeight=math.min(300,height*.46)
    local gaugeWidth=(width-36)/4
    local diameter=math.min(156,gaugeWidth-36,bandHeight-112)
    local function bytes(value)
        if type(value)~="number" then return "—" end
        return value>=1024^3 and string.format("%.1f GiB",value/1024^3) or string.format("%.1f MiB",value/1024^2)
    end
    local icons={cpu="memory",memory="storage",gpu="developer_board",storage="hard_drive"}
    local gauges={}
    for _,metric in ipairs(ui.resolve_for_check(ui.bind("dashboard.gauges"))) do
        local fraction=type(metric.fraction)=="number" and metric.fraction or nil
        local x=(gaugeWidth-diameter)/2
        local reading=metric.id=="gpu" and type(metric.total)=="number" and (bytes(metric.used).." / "..bytes(metric.total)) or bytes(metric.used)
        local caption=metric.id=="cpu" and ui.tr("bead.dashboard.system-temperature")
            or type(metric.total)=="number" and ui.tr("bead.dashboard.system-total",{value=bytes(metric.total)})
            or ui.tr("bead.dashboard.system-unavailable")
        table.insert(gauges,ui.Box{id=metric.id,width=gaugeWidth,height=bandHeight,
            ui.Text{position="absolute",left=14,top=12,text=icons[metric.id] or metric.icon,family="Material Symbols Rounded",
                font_size=20,width=24,height=24,color="primary"},
            ui.Text{position="absolute",left=40,top=13,text=metric.label,width=gaugeWidth-82,height=20,
                font_size=13.333,color="onSurfaceVariant"},
            ui.Arc{id="track",position="absolute",left=x+6,top=42,width=diameter-12,height=diameter-12,
                start_angle=135,sweep_angle=270,stroke_width=9,color=ui.alpha("primary",.14),round_caps=true},
            fraction and ui.Arc{id="amount",position="absolute",left=x+6,top=42,width=diameter-12,height=diameter-12,
                start_angle=135,sweep_angle=270*math.max(0,math.min(1,fraction)),stroke_width=9,color="primary",round_caps=true} or nil,
            ui.Text{id="percent",position="absolute",left=x,top=36+(diameter-44)/2,width=diameter,height=44,
                text=metric.value,font_size=32,font_weight=600,text_align="center",color="primary"},
            ui.Text{id="value",position="absolute",left=14,top=diameter+46,width=gaugeWidth-28,height=28,
                text=reading,font_size=metric.id=="gpu" and 16 or 21.333,font_weight=600,text_align="center",color="onSurface"},
            ui.Text{id="detail",position="absolute",left=14,top=diameter+77,width=gaugeWidth-28,height=20,
                text=caption,font_size=12,text_align="center",color="onSurfaceVariant"},
        })
    end
    local function plot(id,model,color)
        local items=ui.resolve_for_check(ui.bind(model))
        local points={}
        for i,item in ipairs(items) do
            points[i]={(i-1)/math.max(1,#items-1),1-math.clamp(item.h/40,0,1)}
        end
        return ui.Polyline{id=id,position="absolute",left=16,top=82,width=325,height=95,
            points=points,stroke_width=2,color=color,round_caps=true}
    end
    local network=ui.Box{id="network",position="absolute",left=0,top=bandHeight+16,width=357,height=212,
        radius=16,fill="surfaceContainer",
        ui.Text{id="title",position="absolute",left=16,top=14,width=325,height=22,
            text=ui.tr("bead.dashboard.page-network"),font_size=14.667,font_weight=600,color="onSurface"},
        ui.Row{id="rates",position="absolute",left=16,top=58,width=325,height=20,gap=16,
            ui.Text{id="down",text=ui.bind("dashboard.net_rate"),width=145,height=20,font_size=12,color="primary"},
            ui.Text{id="up",text=ui.bind("dashboard.net_up_rate"),width=145,height=20,font_size=12,color="secondary"}},
        plot("down-plot","dashboard.net_down","primary"),
        plot("up-plot","dashboard.net_up","secondary"),
        ui.Text{id="peak",position="absolute",left=16,bottom=12,text=ui.bind("dashboard.net_peak"),width=325,height=18,font_size=12,color="onSurfaceVariant"},
    }
    -- Host card: user, OS, compositor, uptime, kernel from `dashboard.system.host`, three per line.
    local host={{},{}}
    for i,item in ipairs(ui.resolve_for_check(ui.bind("dashboard.host"))) do
        table.insert(i<=3 and host[1] or host[2],ui.Row{id="host-"..item.id,width=144,height=24,gap=4,align="center",
            ui.Text{text=item.icon,family="Material Symbols Rounded",font_size=16,width=20,height=20,color=i%2==0 and "onSurfaceVariant" or "primary"},
            ui.Text{text=item.text,width=120,height=18,font_size=12,color="onSurface"}})
    end
    return ui.Box{id="system",width=940,height=562,
        ui.Box{id="metric-band",position="absolute",left=0,top=0,width=width,height=bandHeight,radius=20,fill="surfaceContainer",
            ui.Row{id="metrics",width=width,height=bandHeight,gap=12,table.unpack(gauges)}},
        network,
        ui.Column{id="processes",position="absolute",left=369,top=274,width=571,height=212,padding=16,gap=4,radius=16,fill="surfaceContainer",
            ui.Text{id="title",text=ui.bind("dashboard.heading"),width=539,height=24,font_size=17.333,font_weight=600,color="onSurface"},
            ui.Column{id="items",width=539,height=156,gap=4,
                ui.Repeat{id="rows",model=ui.bind("dashboard.rows"),key="id",render=function(item)
                    return ui.Row{id="process",width=539,height=21,
                        ui.Text{text=item.label,width=350,height=21,font_size=13.333,color="onSurface"},
                        ui.Text{text=item.value,width=189,height=21,font_size=13.333,text_align="right",color="onSurfaceVariant"}}
                end}},
        },
        ui.Row{id="updates",position="absolute",left=0,top=498,width=940,height=64,padding=12,gap=16,radius=16,fill="surfaceContainer",align="center",
            ui.Text{text="package_2",family="Material Symbols Rounded",font_size=24,width=30,height=30,color="primary"},
            ui.Column{width=390,height=40,gap=0,
                ui.Text{text=ui.bind("dashboard.updates"),width=390,font_size=16,height=22,color="onSurface"},
                ui.Text{text=ui.bind("dashboard.updates_detail"),width=390,font_size=12,height=18,color="onSurfaceVariant"},
            },
        },
        #host[1]>0 and ui.Column{id="host",position="absolute",left=472,top=498,width=468,height=64,padding=8,gap=0,radius=16,fill="transparent",
            ui.Row{id="host-1",width=444,height=24,gap=6,table.unpack(host[1])},
            ui.Row{id="host-2",width=444,height=24,gap=6,table.unpack(host[2])}} or nil,
    }
end
