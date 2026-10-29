-- SPDX-License-Identifier: GPL-3.0-only
local ui = require("bead.ui")
local Entry = ui.component("NavigationEntry", {item={id="string",name="string",icon="string",selected="boolean"}}, function(p)
    return ui.Button {
        id="entry", height=37, radius=9, padding=0, color="onSurface",
        selected=p.item.selected, fill="transparent", selected_fill=ui.tint("surface","onSurface",.12),
        hover_fill=ui.tint("surface","onSurface",.04), border_color=ui.alpha("onSurface",.16),
        on_activate=ui.action("navigate",p.item.id),
        ui.Box{position="absolute",left=0,top=8,width=3,height="60%",radius=1.5,fill="primary",visible=p.item.selected},
        ui.Row{grow=1,gap=10,align="center",padding_left=11,
            ui.Text{text=p.item.icon,family="Material Symbols Rounded",font_size=20,width=20,height=28},
            ui.Text{text=p.item.name,font_size=14,grow=1},
        },
    }
end)
return ui.component("Sidebar", {}, function(_)
    return ui.Column {id="sidebar",width=152,gap=16,
        ui.Row{height=28,gap=9,align="center",
            ui.Image{path="ui/assets/nacre-logo.themed.svg",width=28,height=28,aria_label="Nacre"},
            ui.Text{text="Nacre",font_size=20,grow=1},
        },
        ui.Column{gap=3,
            ui.Repeat{id="navigation",model=ui.bind("launcher.navigation"),key="id",render=function(item)return Entry{item=item}end},
        },
    }
end)
