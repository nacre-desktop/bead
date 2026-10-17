-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local Row=ui.component("ResultRow",{item={id="string",name="string",icon="string",selected="boolean",description="string"}},function(p)
    local item=p.item
    return ui.Button{id="result",width="fill",height="fill",radius=10,padding_left=12,padding_right=12,padding_top=8,padding_bottom=8,fill="transparent",selected=item.selected,
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
    }
end)
return ui.component("Results",{},function(_)
    local viewport=ui.resolve_for_check(ui.bind("bead.viewport.width"))
    local width=math.min(1100,viewport*.9)-12-20-152-18-6
    return kit.VirtualGrid{id="results",scroll=ui.bind("launcher.scroll.y"),width=width,height=400,min_cell_width=width,
        gap_x=0,gap_y=3,row_height=55,total_count=ui.bind("launcher.total_count"),model=ui.bind("launcher.apps"),key="id",render=function(item)return Row{item=item}end}
end)
