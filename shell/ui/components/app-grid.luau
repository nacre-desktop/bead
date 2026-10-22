-- SPDX-License-Identifier: GPL-3.0-only
local ui = require("bead.ui")
local kit = require("bead.kit")
local Tile = ui.component("AppTile", {item={id="string",name="string",icon="string",selected="boolean",favorite="boolean",favoriteId="string"}}, function(p)
    return ui.Box{id="tile-wrap",width="fill",height="fill",
        ui.Button {id="tile",height="fill",width="fill",radius=11,padding=0,
            selected=p.item.selected,fill="transparent",color="onSurface",border_color="outlineVariant",
            selected_fill=ui.tint("surface","onSurface",.09),hover_fill=ui.tint("surface","onSurface",.04),
            on_activate=ui.action("select_app",p.item.id),aria_label=p.item.name,
            ui.Box{position="absolute",left=0,top=10,width=3,height="70%",radius=1.5,fill="primary",visible=p.item.selected},
            ui.Column{align="center",gap=4,padding_top=7,padding_left=7,padding_right=7,
                ui.Image{icon=p.item.icon,width=36,height=36,aria_label=p.item.name},
                ui.Text{text=p.item.name,font_size=13.333,height=28,text_align="center"},
            },
        },
        ui.Button{id="favorite",position="absolute",right=3,top=3,width=26,height=26,radius=8,padding=0,
            aria_label=p.item.favorite and "Remove from favorites" or "Add to favorites",
            fill=p.item.favorite and ui.tint("surface","primary",.12) or "transparent",
            on_activate=ui.action("toggle_favorite",p.item.id),
            ui.Text{text=p.item.favorite and "star" or "star_outline",family="Material Symbols Rounded",width="fill",height="fill",font_size=18,text_align="center",color=p.item.favorite and "primary" or "onSurfaceVariant"},
        },
    }
end)
return ui.component("AppGrid",{},function(_)
    local viewport=ui.resolve_for_check(ui.bind("bead.viewport.width"))
    local width=math.min(1100,viewport*.9)-12-20-152-18-6
    return kit.VirtualGrid{id="app-grid",scroll=ui.bind("launcher.scroll.y"),
        width=width,height=400,min_cell_width=110,gap_x=6,gap_y=5,row_height=76.2,
        model=ui.bind("launcher.apps"),key="id",render=function(item)return Tile{item=item}end}
end)
