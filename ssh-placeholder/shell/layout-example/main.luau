-- SPDX-License-Identifier: GPL-3.0-only
--!strict
local ui = require("bead.ui")
local Behavior = require("./behavior")
local function AppTile(p: { item: Behavior.App }): ui.Node
    return ui.Button {
        id = "tile", text = p.item.name, selected = p.item.selected,
        on_activate = ui.action("select", p.item.id),
    }
end
return ui.Panel {
    id = "launcher", width = 800, height = 480, padding = 12,
    ui.Column { gap = 8,
        ui.TextInput { id = "search", text = ui.bind("launcher.query"),
            placeholder = "Search applications" },
        ui.Row { gap = 12, grow = 1,
            ui.Column { width = "20%", gap = 4,
                ui.Repeat { id = "nav", model = ui.bind("launcher.navigation"), key = "id",
                    render = function(item)
                        return ui.Button { id = "entry", text = item.name, selected = item.selected,
                            on_activate = ui.action("navigate", item.id) }
                    end },
            },
            ui.Scroll { id = "apps-scroll", scroll = ui.bind("launcher.scroll"), grow = 1,
                ui.Grid { columns = 3, gap = 8,
                    ui.Repeat { id = "apps", model = ui.bind("launcher.apps"), key = "id",
                        render = function(item) return AppTile { item = item } end },
                },
            },
        },
    },
}
