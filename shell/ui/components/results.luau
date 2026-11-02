-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local M=require("./metrics")
local Icon=require("./item-icon")
local Lines=require("./text-lines")
local Row=ui.component("ResultRow",{item={id="string",name="string",icon="string",selected="boolean",description="string",favorite="boolean",favoriteId="string"}},function(p)
    local item=p.item
    local width=M.content_width(ui.resolve_for_check(ui.bind("bead.viewport.width")),ui.resolve_for_check(ui.bind("launcher.quick_allowed")))-12-42-M.result_icon-11
    return ui.Box{id="result-wrap",width="fill",height="fill",
      ui.Button{id="result",width="fill",height="fill",radius=10,padding_left=12,padding_right=42,padding_top=8,padding_bottom=8,fill="transparent",selected=item.selected,
        selected_fill=ui.tint("surface","onSurface",.09),hover_fill=ui.tint("surface","onSurface",.04),
        on_activate=ui.action("select_app",item.id),aria_label=item.name,
        ui.Box{position="absolute",left=0,top=9,width=3,height=37,radius=1.5,fill="primary",visible=item.selected},
        ui.Row{gap=11,align="center",grow=1,
            type(item.emoji)=="string" and item.emoji~="" and ui.Text{text=item.emoji,font_size=24,width=M.result_icon,height=M.result_icon} or Icon(item,M.result_icon),
            ui.Column{width=width,gap=3,justify="center",
                Lines{id="name",text=item.name,width=width,font_size=M.result_title_font,line_height=M.result_title_height,max_lines=1,color=item.disabled and "onSurfaceVariant" or "onSurface"},
                Lines{id="description",text=item.description,width=width,font_size=M.result_description_font,line_height=M.result_description_height,max_lines=2,color="onSurfaceVariant"},
            },
        },
      },
      ui.Button{id="favorite",position="absolute",right=12,top=16,width=23,height=23,radius=7,padding=0,
        visible=item.favorite or item.selected or M.hovered(item.id),
        aria_label=item.favorite and ui.tr("bead.launcher.favorite-remove") or ui.tr("bead.launcher.favorite-add"),
        fill="transparent",justify="center",align="center",
        on_activate=ui.action("toggle_favorite",item.id),
        ui.Image{path=item.favorite and "ui/components/assets/favorite-1.themed.svg" or "ui/components/assets/favorite-0.themed.svg",width=11*4/3,height=11*4/3},
      },
    }
end)
return ui.component("Results",{},function(_)
    local viewport=ui.resolve_for_check(ui.bind("bead.viewport.width"))
    local width=M.content_width(viewport,ui.resolve_for_check(ui.bind("launcher.quick_allowed")))
    return kit.VirtualGrid{id="results",scroll=ui.bind("launcher.scroll.y"),width=width,height=400,min_cell_width=width,
        gap_x=0,gap_y=3,row_height=M.result_row_height,total_count=ui.bind("launcher.total_count"),model=ui.bind("launcher.apps"),key="id",render=function(item)return Row{item=item}end}
end)
