-- SPDX-License-Identifier: MIT
local ui=require("bead.ui")
local t=require("./tokens")
local children={id="minimal",width="fill",height="fill",padding=t.spacing,gap=t.spacing,fill="surface",
    ui.Text{id="title",text="Bead minimal shell",font_size=22,height=30,color="primary"},
    ui.Text{id="greeting",text=ui.bind("greeting"),font_size=t.font_size,height=24,color="onSurface"},
    ui.TextInput{id="name",text=ui.bind("name"),placeholder="Type your name",height=t.control_height,
        font_size=t.font_size,padding=8,fill="surface",color="onSurface",border_width=1,border_color="outlineVariant",radius=t.radius},
    ui.Button{id="count",text=ui.bind("counter"),on_activate=ui.action("increment"),height=t.control_height,
        font_size=t.font_size,padding=8,fill="primary",color="onPrimary",radius=t.radius},
}
local index=require("../plugins/index")
for _,entry in index.plugins do
    if entry.loaded then
        for _,widget in entry.widgets do
            local plugin=require("../"..widget.module)({ui=ui,plugin={id=entry.id,settings=entry.values}})
            table.insert(children,ui.plugin_gate(entry.id,plugin.render(t)))
        end
    end
end
return ui.Column(children)
