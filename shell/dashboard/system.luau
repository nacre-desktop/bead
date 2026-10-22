-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    local gauges={}
    for _,metric in ipairs(ui.resolve_for_check(ui.bind("dashboard.gauges"))) do
        table.insert(gauges,ui.Column{id=metric.label,width=226,height=258,padding=20,gap=14,radius=20,fill="surfaceContainer",
            ui.Text{text=metric.icon,family="Material Symbols Rounded",font_size=32,width=40,height=40,color="primary"},
            C.title("title",metric.label,186),
            ui.Text{text=metric.value,font_size=52,height=80,color="onSurface"},
            ui.Text{text=metric.detail,height=40,width=186,font_size=14,color="onSurfaceVariant"},
        })
    end
    return ui.Box{id="system",width=940,height=562,
        ui.Row{id="metrics",position="absolute",left=0,top=0,width=940,height=258,gap=12,table.unpack(gauges)},
        ui.Column{id="disks",position="absolute",left=0,top=274,width=357,height=212,padding=16,gap=10,radius=16,fill="surfaceContainerLow",
            C.title("title","Drives",325),C.rows("items","dashboard.secondary",325,156,true,"No drive data"),
        },
        ui.Column{id="processes",position="absolute",left=369,top=274,width=571,height=212,padding=16,gap=4,radius=16,fill="surfaceContainerLow",
            C.title("title",ui.bind("dashboard.heading"),539),
            C.rows("items","dashboard.rows",539,156,true,"No process data"),
        },
        ui.Row{id="updates",position="absolute",left=0,top=498,width=940,height=64,padding=12,gap=16,radius=16,fill="surfaceContainerLow",align="center",
            ui.Text{text="package_2",family="Material Symbols Rounded",font_size=24,width=30,height=30,color="primary"},
            ui.Column{width=870,height=40,gap=0,
                ui.Text{text=ui.bind("dashboard.updates"),width=870,font_size=16,height=22,color="onSurface"},
                ui.Text{text=ui.bind("dashboard.updates_detail"),width=870,font_size=12,height=18,color="onSurfaceVariant"},
            },
        },
    }
end
