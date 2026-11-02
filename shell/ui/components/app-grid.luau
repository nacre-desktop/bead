-- SPDX-License-Identifier: GPL-3.0-only
local ui = require("bead.ui")
local kit = require("bead.kit")
local M = require("./metrics")
local Icon=require("./item-icon")
local Lines = require("./text-lines")
local Tile = ui.component("AppTile", {item={id="string",name="string",icon="string",selected="boolean",favorite="boolean",favoriteId="string"}}, function(p)
    return ui.Box{id="tile-wrap",width="fill",height="fill",
        ui.Button {id="tile",height="fill",width="fill",radius=11,padding=0,
            selected=p.item.selected,fill="transparent",color="onSurface",border_color="outlineVariant",
            selected_fill=ui.tint("surface","onSurface",.09),hover_fill=ui.tint("surface","onSurface",.04),
            on_activate=ui.action("select_app",p.item.id),aria_label=p.item.name,
            ui.Box{position="absolute",left=0,top=10,width=3,height="70%",radius=1.5,fill="primary",visible=p.item.selected},
            ui.Column{align="center",gap=4,padding_top=7,padding_left=7,padding_right=7,
                Icon(p.item,M.grid_icon),
                Lines{id="name",text=p.item.name,width=M.grid_cell_width-14,font_size=M.grid_label_font,line_height=M.grid_label_height,max_lines=1,text_align="center"},
            },
        },
        ui.Button{id="favorite",position="absolute",right=1,top=3,width=23,height=23,radius=7,padding=0,
            visible=M.hovered(p.item.id) or (not ui.resolve_for_check(ui.bind("launcher.home")) and p.item.selected),
            aria_label=p.item.favorite and ui.tr("bead.launcher.favorite-remove") or ui.tr("bead.launcher.favorite-add"),
            fill="transparent",justify="center",align="center",
            on_activate=ui.action("toggle_favorite",p.item.id),
            ui.Image{path=p.item.favorite and "ui/components/assets/favorite-1.themed.svg" or "ui/components/assets/favorite-0.themed.svg",width=11*4/3,height=11*4/3},
        },
    }
end)
return ui.component("AppGrid",{},function(_)
    local viewport=ui.resolve_for_check(ui.bind("bead.viewport.width"))
    local width=M.content_width(viewport,ui.resolve_for_check(ui.bind("launcher.quick_allowed")))
    if ui.resolve_for_check(ui.bind("launcher.icon_placeholder")) then
        local function row()
            return ui.Row{gap=6,
                ui.Box{width=M.grid_cell_width,height=M.grid_row_height,radius=11,fill=ui.alpha("onSurface",.04)},
                ui.Box{width=M.grid_cell_width,height=M.grid_row_height,radius=11,fill=ui.alpha("onSurface",.04)},
                ui.Box{width=M.grid_cell_width,height=M.grid_row_height,radius=11,fill=ui.alpha("onSurface",.04)},
                ui.Box{width=M.grid_cell_width,height=M.grid_row_height,radius=11,fill=ui.alpha("onSurface",.04)},
            }
        end
        return ui.Column{width=width,height=400,gap=M.grid_gap_y,row(),row()}
    end
    return kit.VirtualGrid{id="app-grid",scroll=ui.bind("launcher.scroll.y"),
        width=width,height=400,min_cell_width=M.grid_cell_width,gap_x=M.grid_gap_x,gap_y=M.grid_gap_y,row_height=M.grid_row_height,
        model=ui.bind("launcher.apps"),key="id",render=function(item)return Tile{item=item}end}
end)
