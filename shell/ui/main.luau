-- SPDX-License-Identifier: GPL-3.0-only
local ui = require("bead.ui")
local Sidebar = require("./components/sidebar")
local AppGrid = require("./components/app-grid")
return ui.Panel {id="launcher",width=1100,height=590,max_width="90%",max_height="90%",
    open=ui.bind("launcher.open"),transition="ease",duration=.2,
    radius=12,fill="surface",border_width=1,border_color="outlineVariant",
    padding_left=12,padding_right=20,padding_top=16,padding_bottom=18,gap=15,
    ui.Row{grow=1,
        Sidebar{},
        ui.Box{width=18,height="fill",
            ui.Box{position="absolute",left=8,top=2,width=1,height="98%",fill="outlineVariant"},
        },
        ui.Column{grow=1,gap=8,
            ui.Text{text=ui.bind("launcher.route_title"),font_size=16,height=32},
            ui.Text{text="Applications",color="onSurfaceVariant",font_size=13.333,height=20},
            AppGrid{},
            ui.Box{height=10},
        },
    },
    ui.TextInput{id="search",text=ui.bind("launcher.search.text"),placeholder="Search everything…",
        height=49,padding=15,radius=10,font_size=16,width="fill"},
}
