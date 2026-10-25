-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local t=require("tokens")
local function centered_text(p)
    local lift=p.family=="Material Symbols Rounded" and 0 or t.text_lift
    p.height=p.font_size*t.line_height+2*lift
    if p.position=="absolute" then p.top=(p.top or 0)-lift end
    return ui.Text(p)
end
local launcher=ui.resolve_for_check(ui.bind("bar.launcher"))
local popouts=ui.resolve_for_check(ui.bind("bar.popouts"))
local ws_on=ui.resolve_for_check(ui.bind("bar.workspace.enabled"))
local tray_on=ui.resolve_for_check(ui.bind("bar.tray.enabled"))
local dash_on=ui.resolve_for_check(ui.bind("bar.dashboard.enabled"))
local focus_on=ui.resolve_for_check(ui.bind("bar.window.focus.enabled"))
local updates_on=ui.resolve_for_check(ui.bind("bar.updates.enabled"))
local updates_n=ui.resolve_for_check(ui.bind("bar.updates.count"))
local header=ui.resolve_for_check(ui.bind("bar.height"))
local title=ui.resolve_for_check(ui.bind("bar.title"))
local title_width=math.min(t.title_max_width,ui.text_width(title,"IBM Plex Sans",t.title_font))
local cue_extent=math.min(t.title_cue_max,title_width+t.icon_width+t.title_gap+10)
local dwell=ui.resolve_for_check(ui.bind("bead.edge.dwell"))
local ws_count=ui.resolve_for_check(ui.bind("bar.workspace.count"))
local ws_icons=ui.resolve_for_check(ui.bind("bar.workspace.icons"))
local thread=true
for _,entry in ws_icons do if entry.icon~="" then thread=false end end
-- QML NacreWorkspaceRow.tint: pearls blend primary -> secondary -> tertiary by position along the row.
local function pearl_accent(at)
    local position=math.min(1,at/math.max(1,ws_count-1))
    if position<=0.5 then return ui.tint("primary","secondary",position*2) end
    return ui.tint("secondary","tertiary",(position-0.5)*2)
end
local function glyph(id,text,color)
    return centered_text{id=id,text=text,width=t.icon_width,shrink=0,family="Material Symbols Rounded",font_size=t.icon_font,color=color or "secondary",text_align="center"}
end
return kit.Bar{edge="top",id="bar",width="100%",height=header,radius=0,fill="surface",padding_left=15,padding_right=15,align="center",justify="space-between",
    ui.Row{id="left",height=34,gap=12,align="center",
        launcher and kit.Button{id="brand",width=t.brand_size,height=t.brand_size,padding=0,fill="transparent",aria_label=ui.tr("bead.bar.open-launcher"),on_activate=ui.action("launcher.open"),ui.Image{path="bar/assets/nacre-logo.themed.svg",width=t.brand_size,height=t.brand_size}} or ui.Image{id="brand",path="bar/assets/nacre-logo.themed.svg",width=t.brand_size,height=t.brand_size},
        ui.Row{id="capsule",height=34,padding_left=5,padding_right=5,gap=2,radius=17,fill="surfaceContainer",align="center",
            -- QML workspaceThread: a 1 px primary -> secondary -> tertiary line at 12 % behind the pearls, inset 16 px.
            thread and kit.tokens.gradients and ui.Image{id="thread",position="absolute",left=21,right=21,top=16.5,height=1,path="bar/assets/workspace-thread.themed.svg"} or ui.Box{id="thread",width=0,height=0,shrink=0,visible=false},
            ui.Repeat{id="workspaces",model=ui.bind("bead.workspaces"),key="id",render=function(item)
                local accent=pearl_accent((item.index or 1)-1)
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
                    button=kit.Button{id="workspace",position="absolute",left=gs,top=0,width=46,height=t.workspace_height,radius=t.workspace_radius,padding=0,fill=on and accent or "surfaceContainer",hover_fill=on and accent or ui.alpha(accent,0.1),shrink=0,
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
                            apps[#apps+1]=centered_text{id="app"..slot,text=glyph_name,width=15,height=28,shrink=0,family="Material Symbols Rounded",font_size=t.app_font,text_align="center",color="onSurface"}
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
                    local size=on and t.pearl_current or lit and t.pearl_occupied or t.pearl_empty
                    local parts={}
                    local function add(node) parts[#parts+1]=node end
                    if on then
                        add(ui.Box{id="active-capsule",position="absolute",left=0,top=(t.workspace_height-t.active_height)/2,width=width,height=t.active_height,radius=t.active_radius,
                            fill=ui.tint("surfaceContainer",accent,.15),border_width=1,border_color=ui.alpha(accent,.24)})
                    end
                    -- QML NacreWorkspacePearl: 22 px current / 20 occupied / 18 empty. The enamel is a flat accent warmed toward
                    -- cream with a neutral cream/secondary shading and specular highlight over it, so any blended accent works.
                    -- Gradients on: the plain accent under the shading; off: the flat warmed colour (QML numberSurface).
                    local shaded=lit and kit.tokens.gradients
                    add(ui.Box{id="pearl",position="absolute",left=16-size/2,top=t.workspace_height/2-size/2,width=size,height=size,radius=size/2,fill=lit and (shaded and accent or ui.tint(accent,t.pearl_cream,on and .4 or .1)) or "surfaceContainer"})
                    if shaded then
                        add(ui.Image{id="enamel-gradient",position="absolute",left=16-size/2,top=t.workspace_height/2-size/2,width=size,height=size,
                            path="bar/assets/pearl-"..(on and "current" or "occupied")..".themed.svg"})
                    end
                    local stroke=on and t.pearl_current_border or not lit and t.pearl_empty_border or 0
                    if stroke>0 then
                        -- Bead strokes straddle their path; Qt's Rectangle border stays inside its box.
                        add(ui.Box{id="ring",position="absolute",left=16-size/2+stroke/2,top=t.workspace_height/2-size/2+stroke/2,width=size-stroke,height=size-stroke,radius=(size-stroke)/2,
                            fill="transparent",border_width=stroke,border_color=on and ui.alpha("onSurface",.5) or ui.alpha(accent,.9)})
                    end
                    add(centered_text{id="number",position="absolute",left=16-size/2,top=t.workspace_height/2-(#label<2 and 11 or 9)*t.line_height/2,text=label,width=size,font_size=#label<2 and 11 or 9,font_weight=on and 600 or 500,family="IBM Plex Sans",text_align="center",color=lit and t.pearl_dark or "onSurfaceVariant"})
                    add(ui.Row{id="apps",position="absolute",left=33,top=0,width=math.max(0,width-33),height=t.workspace_height,gap=3,align="center",visible=#apps>0,table.unpack(apps)})
                    button=kit.Button{id="workspace",position="absolute",left=gs,top=0,width=width,height=t.workspace_height,radius=t.workspace_radius,padding=0,fill="transparent",hover_fill=ui.alpha(accent,.1),shrink=0,
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
                return ui.Box{id="cell",width=gs+width,height=t.workspace_height,shrink=0,table.unpack(kids)}
            end},
        },
    },
    -- QML NacreHoverCue: while the dashboard dwell fills (`bead.edge.dwell`, 0..1 over the hover delay) a 32 px pill
    -- outline and tint fade in (`bead.edge.fade`, 120 ms) and a 2 px trace grows from the centre to both ends.
    -- The 2 px halo margin is part of the box, so everything stays at non-negative offsets.
    ui.Row{id="title-cue",position="absolute",left="28%",right="28%",top=7,height=36,justify="center",visible=ui.bind("bar.title.cue.visible"),
        ui.Box{id="cue",width=cue_extent+4,height=36,radius=18,fill=ui.alpha("primary",ui.bind("bar.title.cue.halo")),
            ui.Box{id="body",position="absolute",left=2,top=2,width=cue_extent,height=32,radius=16,fill=ui.alpha("primary",ui.bind("bar.title.cue.body")),border_width=1,border_color=ui.alpha("primary",ui.bind("bar.title.cue.border"))},
            ui.Box{id="title-trace",position="absolute",left=7+(cue_extent-10)*(1-dwell)/2,top=31,width=(cue_extent-10)*dwell,height=t.title_trace_height,radius=1,fill=ui.alpha("primary",ui.bind("bar.title.cue.fade"))},
        },
    },
    ui.Row{id="title-band",position="absolute",left="28%",right="28%",top=(header-t.title_height)/2,height=t.title_height,justify="center",align="center",gap=t.title_gap,fill="transparent",hover_fill="transparent",focus_ring=false,edge_hover=true,aria_label=ui.tr("bead.bar.active-window"),on_activate=ui.action("bar.title"),
        centered_text{id="title-icon",visible=ui.bind("bead.desktop.titlevisible"),text="desktop_windows",width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,color="primary"},
        centered_text{id="title",visible=ui.bind("bead.desktop.titlevisible"),text=title,height=24,width=title_width,max_width=t.title_max_width,family="IBM Plex Sans",font_size=t.title_font,color="primary"},
        kit.Tooltip{id="tray-tooltip",edge="top",bar_size=-8,left=0,width="100%",height=30,padding=3,visible=ui.bind("bead.desktop.tray.tooltip.visible"),
            ui.Text{text=ui.bind("bead.desktop.tray.tooltip"),height=24,width="100%",font_size=13,color="onSurface"},
        },
        ui.Repeat{id="menu",model=ui.bind("bead.desktop.menu"),key="id",render=function(item)
            return kit.Button{id="entry",text=item.label,height=30,padding=4,font_size=13,on_activate=ui.action("services.menu",item.id)}
        end},
    },
    ui.Row{id="right",height=34,gap=12,shrink=0,align="center",
        ui.Text{id="reconnecting",visible=ui.bind("bar.reconnecting"),text=ui.tr("bead.bar.reconnecting"),width=92,height=24,font_size=12,color="error"},
        ui.Row{id="tray",visible=ui.bind("bar.tray.visible"),height=30,gap=4,align="center",
            ui.Repeat{id="items",model=ui.bind("bead.desktop.tray"),key="id",render=function(item)
                return kit.Button{id="item",width=28,height=28,padding=2,fill=item.attention and "secondaryContainer" or "surface",radius=14,aria_label=item.title,on_activate=tray_on and ui.action("services.tray",item.id) or nil,
                    ui.Image{id="icon",icon=item.icon,width=24,height=24},
                }
            end},
        },
        ui.Row{id="system-pill",height=34,gap=1,shrink=0,padding_left=6,padding_right=6,align="center",fill="surfaceContainer",radius=17,
            on_activate=popouts and ui.action("view.open","control") or nil,
            -- Optional item: unavailable and zero counts occupy no space.
            updates_on and updates_n>0 and kit.Button{id="updates",direction="row",padding=0,fill="transparent",hover_fill="surfaceContainerHigh",on_activate=popouts and ui.action("view.open","dashboard-system") or nil,height=30,gap=4,shrink=0,padding_left=3,padding_right=3,align="center",
                glyph("package","package_2","primary"),
                centered_text{id="count",text=ui.bind("bar.updates.text"),width=t.update_digit_width*#tostring(updates_n)+t.update_text_inset,height=24,font_size=16,color="primary"},
            } or ui.Box{id="updates",width=0,height=0,shrink=0,visible=false},
            ui.Row{id="status",height=34,gap=1,shrink=0,align="center",
            kit.StatusIcon{id="audio",width=30,height=28,padding=0,align="center",justify="center",fill="transparent",hover_fill="surfaceContainerHigh",aria_label=popouts and ui.tr("bead.bar.open-sound") or ui.tr("bead.bar.audio-hint"),on_activate=popouts and ui.action("status.pill.open","audio") or ui.action("services.audio"),
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
            (popouts and kit.StatusIcon or ui.Row){id="battery",direction="row",height=t.status_height,gap=t.item_gap,padding=0,padding_left=t.item_padding,padding_right=t.item_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-battery"),on_activate=popouts and ui.action("status.pill.open","battery") or nil,
                glyph("glyph",ui.bind("bead.desktop.battery")),
                centered_text{id="battery-value",text=ui.bind("bead.desktop.battery.value"),width=ui.bind("bar.battery.detail.width"),font_size=t.detail_font,color="secondary"},
            },
            (popouts and kit.StatusIcon or ui.Row){id="notifications",direction="row",height=t.status_height,gap=t.item_gap,padding=0,padding_left=t.item_padding,padding_right=t.item_padding,fill="transparent",hover_fill="surfaceContainerHigh",align="center",justify="center",
                aria_label=ui.tr("bead.bar.open-notifications"),on_activate=popouts and ui.action("status.pill.open","notifications") or nil,
                glyph("glyph","notifications"),
                centered_text{id="notifications-count",text=ui.bind("bar.notifications.text"),width=ui.bind("bar.notifications.detail.width"),font_size=t.detail_font,color="secondary"},
            },
            },
            (popouts and kit.Button or ui.Row){id="clock",height=30,direction=popouts and "row" or nil,gap=4,padding_left=3,padding_right=3,align="center",fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-calendar") or nil,on_activate=popouts and ui.action("view.open","calendar") or nil,
                glyph("calendar","calendar_month","tertiary"),
                centered_text{id="time",text=ui.bind("bar.clock.text"),width=ui.bind("bar.clock.width"),shrink=0,font_size=16,height=24,family="IBM Plex Sans",color="tertiary"},
            },
            (popouts and kit.Button or ui.Row){id="power",height=30,width=30,justify="center",direction=popouts and "row" or nil,align="center",padding=popouts and 0 or nil,fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-session") or nil,on_activate=popouts and ui.action("view.open","session") or nil,
                glyph("glyph","power_settings_new","error"),
            },
        },
    },
}
