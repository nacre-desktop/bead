-- SPDX-License-Identifier: GPL-3.0-only
local ui = require("bead.ui")
local kit = require("bead.kit")
local Sidebar = require("./components/sidebar")
local AppGrid = require("./components/app-grid")
local Results = require("./components/results")
local has_detail = ui.resolve_for_check(ui.bind("launcher.has_detail"))
local detail_only = has_detail and ui.resolve_for_check(ui.bind("launcher.result_count")) == 0
local detail = ui.Box{visible=false,height=0}
if has_detail then
    local value=ui.resolve_for_check(ui.bind("launcher.conversion.value"))
    local description=ui.resolve_for_check(ui.bind("launcher.conversion.description"))
    local error=ui.resolve_for_check(ui.bind("launcher.conversion.error"))
    local message=ui.resolve_for_check(ui.bind("launcher.message"))
    detail=ui.Column{id="engine-detail",gap=6,padding_top=12,padding_bottom=12,
        ui.Text{id="conversion-value",text=value,visible=value~="",font_size=24,height=72},
        ui.Text{text=description,visible=description~="",font_size=13.333,height=20,color="onSurfaceVariant"},
        ui.Text{text=error,visible=error~="",font_size=13.333,height=40,color="error"},
        ui.Text{id="engine-guidance",text=message,visible=message~="",font_size=13.333,height=40,color="onSurfaceVariant"},
    }
end
return kit.Panel {edge="top",id="launcher",width=1100,height=590,max_width="90%",max_height="90%",
    open=ui.bind("panel.open"),transition="ease",duration=.2,
    radius=12,fill="surface",border_width=1,border_color="outlineVariant",
    padding_left=12,padding_right=20,padding_top=16,padding_bottom=18,gap=15,
    ui.Row{grow=1,
        Sidebar{},
        ui.Box{width=18,height="fill",
            ui.Box{position="absolute",left=8,top=2,width=1,height="98%",fill="outlineVariant"},
        },
        ui.Column{grow=1,gap=8,
            ui.Text{text=ui.bind("launcher.route_title"),font_size=16,height=32},
            ui.Text{text=ui.bind("launcher.subtitle"),color="onSurfaceVariant",font_size=13.333,height=20},
            ui.Text{id="source-error",text=ui.bind("launcher.error"),visible=ui.bind("launcher.failed"),color="error",font_size=13.333,height=20},
            detail,
            ui.Text{id="no-results",text=ui.tr("bead.launcher.no-results"),visible=ui.bind("launcher.empty"),color="onSurfaceVariant",font_size=13.333,height=20},
            detail_only and ui.Box{grow=1} or ui.resolve_for_check(ui.bind("launcher.grid"))
                and ui.Box{id="grid-host",grow=1,AppGrid{}}
                or ui.Box{id="results-host",grow=1,Results{}},
            ui.Box{height=10},
        },
    },
    kit.TextInput{id="search",text=ui.bind("launcher.search.text"),placeholder=ui.tr("bead.launcher.search-placeholder"),
        height=49,padding=15,radius=10,font_size=16,width="fill"},
}
