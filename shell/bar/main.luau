-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local launcher=ui.resolve_for_check(ui.bind("bar.launcher"))
local popouts=ui.resolve_for_check(ui.bind("bar.popouts"))
local ws_on=ui.resolve_for_check(ui.bind("bar.workspace.enabled"))
local tray_on=ui.resolve_for_check(ui.bind("bar.tray.enabled"))
local function glyph(id,text,color)
    return ui.Text{id=id,text=text,width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,color=color or "secondary",text_align="center"}
end
return kit.Bar{edge="top",id="bar",width="100%",height=50,radius=0,fill="surface",padding_left=15,padding_right=15,align="center",justify="space-between",
    ui.Row{id="left",height=34,gap=12,align="center",
        launcher and kit.Button{id="brand",width=30,height=30,padding=0,fill="transparent",aria_label=ui.tr("bead.bar.open-launcher"),on_activate=ui.action("launcher.open"),ui.Image{path="bar/assets/nacre-logo.svg",width=30,height=30}} or ui.Image{id="brand",path="bar/assets/nacre-logo.svg",width=30,height=30},
        ui.Row{id="capsule",height=34,padding_left=5,padding_right=5,gap=2,radius=17,fill="surfaceContainer",align="center",
            ui.Repeat{id="workspaces",model=ui.bind("bead.workspaces"),key="id",render=function(item)
                local accent=({"primary","secondary","tertiary"})[((item.index or 1)-1)%3+1]
                return kit.Button{id="workspace",width=32,height=28,radius=14,padding=0,fill="surfaceContainer",shrink=0,
                    on_activate=ws_on and ui.action("backend.workspace.switch",item.id) or nil,
                    ui.Box{id="halo",position="absolute",left=3,top=1,width=26,height=26,radius=13,fill=item.active and ui.alpha(accent,0.3) or "surfaceContainer"},
                    ui.Box{id="pearl",position="absolute",left=6,top=4,width=20,height=20,radius=10,border_width=item.active and 0 or 1,border_color=accent,fill=item.active and accent or "surfaceContainer"},
                    ui.Text{id="number",position="absolute",left=6,top=5,text=item.name,width=20,height=20,font_size=13,family="IBM Plex Sans",text_align="center",color=item.active and "onPrimary" or "onSurfaceVariant"},
                }
            end},
        },
        popouts and kit.Button{id="dashboard",width=30,height=30,padding=0,fill="transparent",aria_label=ui.tr("bead.bar.open-dashboard"),on_activate=ui.action("view.open","dashboard-today"),
            glyph("glyph","dashboard","primary"),
        } or nil,
    },
    ui.Row{id="title-band",position="absolute",left="28%",right="28%",top=10,height=30,justify="center",align="center",gap=7,aria_label=ui.tr("bead.bar.active-window"),on_activate=ui.action("bar.title"),
        ui.Text{id="title-icon",visible=ui.bind("bead.desktop.titlevisible"),text="desktop_windows",width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,color="primary"},
        ui.Text{id="title",visible=ui.bind("bead.desktop.titlevisible"),text=ui.bind("bead.focused_title"),height=24,width=ui.bind("bar.title.width"),max_width=550,family="JetBrainsMono Nerd Font",font_size=16,color="primary"},
        kit.Tooltip{id="tray-tooltip",edge="top",bar_size=-8,left=0,width="100%",height=30,padding=3,visible=ui.bind("bead.desktop.tray.tooltip.visible"),
            ui.Text{text=ui.bind("bead.desktop.tray.tooltip"),height=24,width="100%",font_size=13,color="onSurface"},
        },
        ui.Repeat{id="menu",model=ui.bind("bead.desktop.menu"),key="id",render=function(item)
            return kit.Button{id="entry",text=item.label,height=30,padding=4,font_size=13,on_activate=ui.action("services.menu",item.id)}
        end},
    },
    ui.Row{id="right",height=34,gap=12,shrink=0,align="center",
        ui.Text{id="reconnecting",visible=ui.bind("bar.reconnecting"),text=ui.tr("bead.bar.reconnecting"),width=92,height=24,font_size=12,color="error"},
        ui.Row{id="tray",height=30,gap=4,align="center",
            ui.Repeat{id="items",model=ui.bind("bead.desktop.tray"),key="id",render=function(item)
                return kit.Button{id="item",width=28,height=28,padding=2,fill=item.attention and "secondaryContainer" or "surface",radius=14,aria_label=item.title,on_activate=tray_on and ui.action("services.tray",item.id) or nil,
                    ui.Image{id="icon",icon=item.icon,width=24,height=24},
                }
            end},
        },
        ui.Row{id="system-pill",height=34,gap=1,shrink=0,padding_left=6,padding_right=6,align="center",fill="surfaceContainer",radius=17,
            ui.Row{id="updates",height=34,gap=4,shrink=0,align="center",
                glyph("package","package_2","primary"),
                ui.Text{id="count",text="0",width=10,height=24,font_size=16,color="primary"},
            },
            ui.Row{id="status",height=34,gap=2,shrink=0,padding_left=4,padding_right=4,align="center",
            kit.StatusIcon{id="audio",width=24,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",aria_label=popouts and ui.tr("bead.bar.open-sound") or ui.tr("bead.bar.audio-hint"),on_activate=popouts and ui.action("status.pill.open","audio") or ui.action("services.audio"),
                glyph("glyph",ui.bind("bead.desktop.audio")),
            },
            popouts and kit.StatusIcon{id="network",width=24,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-network"),on_activate=ui.action("status.pill.open","network"),
                glyph("glyph",ui.bind("bead.desktop.network")),
            } or glyph("network",ui.bind("bead.desktop.network")),
            popouts and kit.StatusIcon{id="bluetooth",width=24,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-bluetooth"),on_activate=ui.action("status.pill.open","bluetooth"),
                glyph("glyph",ui.bind("bead.desktop.bluetooth")),
            } or glyph("bluetooth",ui.bind("bead.desktop.bluetooth")),
            popouts and kit.StatusIcon{id="battery",width=24,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-battery"),on_activate=ui.action("status.pill.open","battery"),
                glyph("glyph",ui.bind("bead.desktop.battery")),
            } or glyph("battery",ui.bind("bead.desktop.battery")),
            ui.Text{id="battery-value",text=ui.bind("bead.desktop.battery.value"),height=20,width=24,shrink=0,font_size=13,color="secondary"},
            },
            (popouts and kit.Button or ui.Row){id="clock",height=30,direction=popouts and "row" or nil,gap=7,align="center",padding=popouts and 0 or nil,fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-calendar") or nil,on_activate=popouts and ui.action("view.open","calendar") or nil,
                glyph("calendar","calendar_month","tertiary"),
                ui.Text{id="time",text=ui.bind("bead.desktop.clock"),width=48,shrink=0,font_size=16,height=24,family="IBM Plex Sans",color="tertiary"},
            },
            (popouts and kit.Button or ui.Row){id="power",height=30,direction=popouts and "row" or nil,align="center",padding=popouts and 0 or nil,fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-session") or nil,on_activate=popouts and ui.action("view.open","session") or nil,
                glyph("glyph","power_settings_new","error"),
            },
        },
    },
}
