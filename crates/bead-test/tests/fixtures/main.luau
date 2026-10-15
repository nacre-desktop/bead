-- SPDX-License-Identifier: MIT
local ui = require("bead.ui")
return ui.Panel {id="test",width=320,height=180,padding=12,fill="surface",radius=14,
    ui.Column {gap=8,
        ui.Text {id="title",text="Bead foundation",height=24,font_size=18,color="onSurface"},
        ui.TextInput {id="query",text=ui.bind("query"),placeholder="Search",height=36},
        ui.Button {id="apply",text="Apply",height=36,on_activate=ui.action("apply")},
        ui.Text {id="count",text=ui.bind("count"),height=20,color="onSurface"},
    },
}
