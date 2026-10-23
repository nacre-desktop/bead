-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local launcher=ui.resolve_for_check(ui.bind("bar.launcher"))
local popouts=ui.resolve_for_check(ui.bind("bar.popouts"))
local ws_on=ui.resolve_for_check(ui.bind("bar.workspace.enabled"))
local tray_on=ui.resolve_for_check(ui.bind("bar.tray.enabled"))
local dash_on=ui.resolve_for_check(ui.bind("bar.dashboard.enabled"))
local focus_on=ui.resolve_for_check(ui.bind("bar.window.focus.enabled"))
local header=ui.resolve_for_check(ui.bind("bar.height"))
local function glyph(id,text,color)
    return ui.Text{id=id,text=text,width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,color=color or "secondary",text_align="center"}
end
return kit.Bar{edge="top",id="bar",width="100%",height=header,radius=0,fill="surface",padding_left=15,padding_right=15,align="center",justify="space-between",
    ui.Row{id="left",height=34,gap=12,align="center",
        launcher and kit.Button{id="brand",width=30,height=30,padding=0,fill="transparent",aria_label=ui.tr("bead.bar.open-launcher"),on_activate=ui.action("launcher.open"),ui.Image{path="bar/assets/nacre-logo.themed.svg",width=30,height=30}} or ui.Image{id="brand",path="bar/assets/nacre-logo.themed.svg",width=30,height=30},
        ui.Row{id="capsule",height=34,padding_left=5,padding_right=5,gap=2,radius=17,fill="surfaceContainer",align="center",
            ui.Repeat{id="workspaces",model=ui.bind("bead.workspaces"),key="id",render=function(item)
                local accent=({"primary","secondary","tertiary"})[((item.index or 1)-1)%3+1]
                local label=item.label or item.name or ""
                local on=item.active
                local activate=ws_on and ui.action("backend.workspace.switch",item.id) or nil
                -- A visiting workspace group starts with its home output's name (QML groupSpace, 48 px).
                local gs=item.group_start==true and 48 or 0
                local button
                local width
                local overlays={}
                if item.icon_mode then
                    -- workspaceIcons: 46 px cells, accent fill when active, symbol and number, dot when occupied.
                    local ink=on and "onPrimary" or "onSurfaceVariant"
                    local icon=item.icon or ""
                    width=46
                    button=kit.Button{id="workspace",position="absolute",left=gs,top=0,width=46,height=28,radius=14,padding=0,fill=on and accent or "surfaceContainer",hover_fill=on and accent or ui.alpha(accent,0.1),shrink=0,
                        on_activate=activate,
                        ui.Row{id="face",position="absolute",left=0,top=2,width=46,height=24,gap=4,align="center",justify="center",
                            ui.Text{id="symbol",visible=icon~="",text=icon,width=icon~="" and 18 or 0,height=24,shrink=0,family="Material Symbols Rounded",font_size=15,text_align="center",color=ink},
                            ui.Text{id="number",text=label,width=8*#label,height=24,shrink=0,font_size=13,family="IBM Plex Sans",text_align="center",color=ink},
                        },
                        ui.Box{id="occupied",position="absolute",left=21,top=23,width=3,height=3,radius=2,visible=item.occupied==true,fill=on and "onPrimary" or accent},
                    }
                else
                    -- workspaceWindowHints: category glyphs of the active workspace's windows beside its pearl.
                    local shown=item.shown or 0
                    local more=item.overflow or ""
                    local hinted=item.hinted==true
                    local apps={}
                    for slot=1,3 do
                        local glyph_name=item["app"..slot]
                        if glyph_name and glyph_name~="" then
                            apps[#apps+1]=ui.Text{id="app"..slot,text=glyph_name,width=15,height=28,shrink=0,family="Material Symbols Rounded",font_size=14,text_align="center",color="onSurface"}
                            -- Clicking a glyph focuses its window (window.focus); a sibling, as a button cannot nest in a button.
                            local window=item["win"..slot]
                            if focus_on and window and window~="" then
                                overlays[#overlays+1]=kit.Button{id="focus"..slot,position="absolute",left=gs+33+(slot-1)*18,top=0,width=15,height=28,radius=7,padding=0,fill="transparent",hover_fill=ui.alpha("onSurface",0.14),shrink=0,
                                    aria_label=ui.tr("bead.bar.focus-window"),on_activate=ui.action("backend.window.focus",window),
                                    ui.Box{id="hit",width=15,height=28}}
                            end
                        end
                    end
                    if more~="" then
                        apps[#apps+1]=ui.Text{id="more",text=more,width=17,height=28,shrink=0,font_size=12,color="onSurface"}
                    end
                    width=32+shown*18+(more~="" and 20 or 0)+(hinted and 4 or 0)
                    local lit=on or item.occupied==true
                    local size=on and 22 or lit and 20 or 18
                    local parts={}
                    local function add(node) parts[#parts+1]=node end
                    -- QML NacreWorkspacePearl: 22 px current / 20 occupied / 18 empty; enamel (accent warmed toward cream) with a specular highlight, ring in onSurface (current) or accent (empty).
                    add(ui.Box{id="pearl",position="absolute",left=16-size/2,top=14-size/2,width=size,height=size,radius=size/2,fill=lit and accent or "surfaceContainer",
                        border_width=on and 1 or lit and 0 or 1.5,border_color=on and ui.alpha("onSurface",0.5) or ui.alpha(accent,0.9)})
                    if lit then
                        add(ui.Box{id="enamel",position="absolute",left=16-size/2,top=14-size/2,width=size,height=size,radius=size/2,fill=ui.alpha("inverseSurface",on and 0.4 or 0.1)})
                        -- the QML enamel runs light (top) to deep (bottom): a sheen over the upper half
                        add(ui.Box{id="sheen",position="absolute",left=16-size/2,top=14-size/2,width=size,height=size*.55,radius=size*.275,fill=ui.alpha("inverseSurface",on and 0.3 or 0.12)})
                        add(ui.Box{id="specular",position="absolute",left=16-size/2+size*.23,top=14-size/2+size*.14,width=size*.44,height=size*.23,radius=size*.115,fill=ui.alpha("inverseSurface",on and 0.6 or 0.3)})
                    end
                    add(ui.Text{id="number",position="absolute",left=16-size/2,top=14-size/2+1.5,text=label,width=size,height=size,font_size=#label<2 and 11 or 9,family="IBM Plex Sans",text_align="center",color=lit and "onPrimary" or "onSurfaceVariant"})
                    add(ui.Row{id="apps",position="absolute",left=33,top=0,width=math.max(0,width-33),height=28,gap=3,align="center",visible=#apps>0,table.unpack(apps)})
                    local face=hinted and ui.tint("surfaceContainer",accent,0.15) or "surfaceContainer"
                    button=kit.Button{id="workspace",position="absolute",left=gs,top=0,width=width,height=28,radius=14,padding=0,fill=face,hover_fill=hinted and face or "surfaceContainerHigh",shrink=0,
                        border_width=hinted and 1 or 0,border_color=ui.alpha(accent,0.24),
                        on_activate=activate,
                        table.unpack(parts),
                    }
                end
                local kids={}
                if gs>0 then
                    kids[#kids+1]=ui.Box{id="group-rule",position="absolute",left=5,top=5,width=1,height=18,fill="outlineVariant"}
                    kids[#kids+1]=ui.Text{id="group-origin",position="absolute",left=10,top=5,width=36,height=18,text="▣ "..(item.origin or ""),font_size=11,color="onSurfaceVariant"}
                end
                kids[#kids+1]=button
                for _,overlay in ipairs(overlays) do kids[#kids+1]=overlay end
                return ui.Box{id="cell",width=gs+width,height=28,shrink=0,table.unpack(kids)}
            end},
        },
    },
    ui.Row{id="title-band",position="absolute",left="28%",right="28%",top=10,height=30,justify="center",align="center",gap=7,aria_label=ui.tr("bead.bar.active-window"),on_activate=ui.action("bar.title"),
        ui.Text{id="title-icon",visible=ui.bind("bead.desktop.titlevisible"),text="desktop_windows",width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,color="primary"},
        ui.Text{id="title",visible=ui.bind("bead.desktop.titlevisible"),text=ui.bind("bar.title"),height=24,width=ui.bind("bar.title.width"),max_width=550,family="IBM Plex Sans",font_size=16,color="primary"},
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
            ui.Row{id="updates",height=34,gap=4,shrink=0,padding_left=3,padding_right=3,align="center",
                glyph("package","package_2","primary"),
                ui.Text{id="count",text="0",width=10,height=24,font_size=16,color="primary"},
            },
            ui.Row{id="status",height=34,gap=1,shrink=0,align="center",
            kit.StatusIcon{id="audio",width=30,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",aria_label=popouts and ui.tr("bead.bar.open-sound") or ui.tr("bead.bar.audio-hint"),on_activate=popouts and ui.action("status.pill.open","audio") or ui.action("services.audio"),
                glyph("glyph",ui.bind("bead.desktop.audio")),
            },
            popouts and kit.StatusIcon{id="network",width=30,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-network"),on_activate=ui.action("status.pill.open","network"),
                glyph("glyph",ui.bind("bead.desktop.network")),
            } or glyph("network",ui.bind("bead.desktop.network")),
            popouts and kit.StatusIcon{id="bluetooth",width=30,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-bluetooth"),on_activate=ui.action("status.pill.open","bluetooth"),
                glyph("glyph",ui.bind("bead.desktop.bluetooth")),
            } or glyph("bluetooth",ui.bind("bead.desktop.bluetooth")),
            popouts and kit.StatusIcon{id="battery",width=27,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-battery"),on_activate=ui.action("status.pill.open","battery"),
                glyph("glyph",ui.bind("bead.desktop.battery")),
            } or glyph("battery",ui.bind("bead.desktop.battery")),
            ui.Row{id="battery-tail",height=28,width=ui.bind("bead.desktop.battery.value.width"),shrink=0,padding_left=4,align="center",
                ui.Text{id="battery-value",text=ui.bind("bead.desktop.battery.value"),height=20,width="fill",font_size=13,color="secondary"},
            },
            -- QML notificationsItem: bell plus the retained count (at most 999), opens the control centre.
            popouts and kit.StatusIcon{id="notifications",width=27,height=28,padding=0,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-notifications"),on_activate=ui.action("view.open","control"),
                glyph("glyph","notifications"),
            } or glyph("notifications","notifications"),
            ui.Row{id="notifications-tail",height=28,width=ui.bind("bar.notifications.tail"),shrink=0,padding_left=4,align="center",
                ui.Text{id="notifications-count",text=ui.bind("bar.notifications.text"),height=20,width="fill",font_size=13,color="secondary"},
            },
            },
            (popouts and kit.Button or ui.Row){id="clock",height=30,direction=popouts and "row" or nil,gap=4,padding_left=3,padding_right=3,align="center",fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-calendar") or nil,on_activate=popouts and ui.action("view.open","calendar") or nil,
                glyph("calendar","calendar_month","tertiary"),
                ui.Text{id="time",text=ui.bind("bar.clock.text"),width=ui.bind("bar.clock.width"),shrink=0,font_size=16,height=24,family="IBM Plex Sans",color="tertiary"},
            },
            (popouts and kit.Button or ui.Row){id="power",height=30,width=30,justify="center",direction=popouts and "row" or nil,align="center",padding=popouts and 0 or nil,fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-session") or nil,on_activate=popouts and ui.action("view.open","session") or nil,
                glyph("glyph","power_settings_new","error"),
            },
        },
    },
}
