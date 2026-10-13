-- SPDX-License-Identifier: GPL-3.0-only
local ui = require("bead.ui")
local Tile = ui.component("AppTile", {item={id="string",name="string",icon="string",selected="boolean"}}, function(p)
    return ui.Button {id="tile",height="fill",width="fill",radius=11,padding=0,
        selected=p.item.selected,fill="transparent",color="onSurface",border_color="outlineVariant",
        selected_fill=ui.tint("surface","onSurface",.09),hover_fill=ui.tint("surface","onSurface",.04),
        on_activate=ui.action("select_app",p.item.id),aria_label=p.item.name,
        ui.Box{position="absolute",left=0,top=10,width=3,height="70%",radius=1.5,fill="primary",visible=p.item.selected},
        ui.Column{align="center",gap=4,padding_top=7,padding_left=7,padding_right=7,
            ui.Image{icon=p.item.icon,width=36,height=36,aria_label=p.item.name},
            ui.Text{text=p.item.name,font_size=13.333,height=28,text_align="center"},
        },
    }
end)
return ui.component("AppGrid",{},function(_)
    return ui.Scroll{id="app-grid",scroll=ui.bind("launcher.scroll.y"),grow=1,
        ui.Grid{min_cell_width=110,gap_x=6,gap_y=5,padding_right=6,padding_bottom=5,row_height=76.2,
            ui.Repeat{id="apps",model=ui.bind("launcher.apps"),key="id",render=function(item)return Tile{item=item}end},
        },
    }
end)
