-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local M=require("./metrics")
local Row=ui.component("ResultRow",{item={id="string",name="string",icon="string",selected="boolean",description="string",favorite="boolean",favoriteId="string"}},function(p)
    local item=p.item
    return ui.Box{id="result-wrap",width="fill",height="fill",
      ui.Button{id="result",width="fill",height="fill",radius=10,padding_left=12,padding_right=42,padding_top=8,padding_bottom=8,fill="transparent",selected=item.selected,
        selected_fill=ui.tint("surface","onSurface",.09),hover_fill=ui.tint("surface","onSurface",.04),
        on_activate=ui.action("select_app",item.id),aria_label=item.name,
        ui.Box{position="absolute",left=0,top=9,width=3,height=37,radius=1.5,fill="primary",visible=item.selected},
        ui.Row{gap=11,align="center",grow=1,
            type(item.emoji)=="string" and item.emoji~="" and ui.Text{text=item.emoji,font_size=24,width=30,height=30} or ui.Image{icon=item.icon,width=30,height=30,aria_label=item.name},
            ui.Column{grow=1,gap=3,justify="center",
                ui.Text{text=item.name,font_size=14.667,height=20,color=item.disabled and "onSurfaceVariant" or "onSurface"},
                ui.Text{text=item.description,font_size=12,color="onSurfaceVariant",height=16},
            },
        },
      },
      ui.Button{id="favorite",position="absolute",right=8,top=10,width=28,height=28,radius=8,padding=0,
        aria_label=item.favorite and ui.tr("bead.launcher.favorite-remove") or ui.tr("bead.launcher.favorite-add"),
        fill=item.favorite and ui.tint("surface","primary",.12) or "transparent",
        on_activate=ui.action("toggle_favorite",item.id),
        ui.Text{text=item.favorite and "star" or "star_outline",family="Material Symbols Rounded",width="fill",height="fill",font_size=19,text_align="center",color=item.favorite and "primary" or "onSurfaceVariant"},
      },
    }
end)
return ui.component("Results",{},function(_)
    local viewport=ui.resolve_for_check(ui.bind("bead.viewport.width"))
    local width=M.content_width(viewport,ui.resolve_for_check(ui.bind("launcher.search.text")))
    return kit.VirtualGrid{id="results",scroll=ui.bind("launcher.scroll.y"),width=width,height=400,min_cell_width=width,
        gap_x=0,gap_y=3,row_height=55,total_count=ui.bind("launcher.total_count"),model=ui.bind("launcher.apps"),key="id",render=function(item)return Row{item=item}end}
end)
