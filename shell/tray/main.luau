-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local height=ui.resolve_for_check(ui.bind("tray.height"))
return kit.Panel{id="tray-panel",edge="top",open=ui.bind("panel.open"),width=300,height=height,padding=0,
    kit.Popover{id="tray-menu",edge="top",bar_size=-8,width=300,height=height,padding=12,gap=4,
        ui.Row{height=32,width="100%",align="center",gap=4,
            kit.Button{id="back",text="‹",width=30,height=28,visible=ui.bind("tray.back"),on_activate=ui.action("services.menu.back")},
            ui.Text{id="title",text=ui.bind("tray.title"),height=24,width=204,font_size=14,color="onSurface"},
            kit.Button{id="close",text="×",aria_label=ui.tr("bead.panel.tray-close"),width=28,height=28,on_activate=ui.action("close")},
        },
        ui.Text{visible=ui.bind("tray.empty"),text=ui.tr("bead.panel.tray-empty"),height=28,font_size=14,color="onSurfaceVariant"},
        ui.Scroll{id="rows",width="100%",height=height-56,scroll=ui.bind("tray.scroll"),
          ui.Column{width="100%",shrink=0,gap=2,
            ui.Repeat{id="entries",model=ui.bind("tray.rows"),key="id",render=function(item)
                if item.separator then return ui.Box{width="100%",height=1,fill="outlineVariant"} end
                return kit.Button{id="entry",width="100%",height=30,padding=4,
                    fill="transparent",on_activate=item.enabled and ui.action(item.submenu and "services.menu.open" or "services.menu",item.id) or nil,
                    ui.Text{text=item.toggle..item.label..(item.submenu and "  ›" or ""),width="100%",height=24,font_size=14,color=item.enabled and "onSurface" or "onSurfaceVariant"},
                }
            end},
          },
        },
    },
}
