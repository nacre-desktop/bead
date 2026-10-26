-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local measure=require("../measure")
local centered_text=c.centered_text
local header=c.header
local title=c.title
local title_width=c.title_width
local dwell=c.dwell
local title_min_width=math.min(t.title_max_width,t.title_height*4)
local displays={"window title","app + title","app only","icon"}
local function active_app()
    local windows=ui.resolve_for_check(ui.bind("bead.windows")) or {}
    for _,window in windows do
        if window.focused then return window.app_id or "" end
    end
    return ""
end
return {id="title",title="Title",icon="title",category="bar",data={topics={},actions={}},
    settings={
        min_width={type="number",min=0,max=t.title_max_width,step=8},
        max_width={type="number",min=128,max=t.title_max_width,step=8},
        display={type="choice",options=displays},
        behavior={type="choice",options={"hover","click","none"}},
    },
    defaults={min_width=title_min_width,max_width=t.title_max_width,display="window title",behavior="hover"},
    budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="window-title",styles={["window-title"]={title="Window title",min_span=1,max_span=math.ceil((t.title_max_width+t.icon_width+t.title_gap)/32),preferred_span=math.ceil((t.title_max_width+t.icon_width+t.title_gap)/32),reserve_width=function(settings) return (settings.max_width or t.title_max_width)+t.icon_width+t.title_gap end,render=function(props,settings,data)
            settings=settings or {}
            local max_width=settings.max_width or t.title_max_width
            local min_width=math.min(settings.min_width or title_min_width,max_width)
            local display=settings.display or "window title"
            local behavior=settings.behavior or "hover"
            -- Keep today's bar path byte-for-byte equivalent for the Nacre defaults.
            assert(min_width<=max_width,"title minimum width exceeds maximum")
            if min_width==title_min_width and max_width==t.title_max_width and display=="window title" and behavior=="hover"
                and not props.local_layout and props.instance.host=="top_bar" and props.instance.slot.region=="centre" then
            local animated_width=ui.animate("title.width."..props.instance.iid,title_width,.18,"out-cubic")
            local animated_cue=math.min(t.title_cue_max,animated_width+t.icon_width+t.title_gap+10)
            -- Keep the bound title reactive; resolve its text only when it exceeds
            -- the configured max-width clipping bound and needs an ellipsis.
            local shown=title
            if ui.text_width(title,"IBM Plex Sans",t.title_font)>animated_width then
                shown=measure.ellipsize(ui.resolve_for_check(title) or "",animated_width,"IBM Plex Sans",t.title_font)
            end
            return ui.Group{ui.Row{id="title-cue",position="absolute",left="28%",right="28%",top=7,height=36,justify="center",visible=ui.bind("bar.title.cue.visible"),
        ui.Box{id="cue",width=animated_cue+4,height=36,radius=18,fill=ui.alpha("primary",ui.bind("bar.title.cue.halo")),
            ui.Box{id="body",position="absolute",left=2,top=2,width=animated_cue,height=32,radius=16,fill=ui.alpha("primary",ui.bind("bar.title.cue.body")),border_width=1,border_color=ui.alpha("primary",ui.bind("bar.title.cue.border"))},
            ui.Box{id="title-trace",position="absolute",left=7+(animated_cue-10)*(1-dwell)/2,top=31,width=(animated_cue-10)*dwell,height=t.title_trace_height,radius=1,fill=ui.alpha("primary",ui.bind("bar.title.cue.fade"))},
        },
    },
    ui.Button{id="title-band",position="absolute",left="28%",right="28%",top=(header-t.title_height)/2,height=t.title_height,direction="row",justify="center",align="center",fill="transparent",hover_fill="transparent",focus_ring=false,focus_on_pointer=false,activate_on_release=true,edge_hover=true,aria_label=ui.tr("bead.bar.active-window"),on_activate=ui.action("bar.title"),
        ui.Row{id="title-content",width=animated_width+t.icon_width+t.title_gap,height=t.title_height,justify="center",align="center",gap=t.title_gap,drag_payload=props.instance.iid,
            centered_text{id="title-icon",visible=ui.bind("bead.desktop.titlevisible"),text="desktop_windows",width=t.icon_width,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,color="primary"},
            centered_text{id="title",visible=ui.bind("bead.desktop.titlevisible"),text=shown,height=24,width=animated_width,max_width=t.title_max_width,family="IBM Plex Sans",font_size=t.title_font,text_align="center",color="primary"},
        },
        kit.Tooltip{id="tray-tooltip",edge="top",bar_size=-8,left=0,width="100%",height=30,padding=3,visible=ui.bind("bead.desktop.tray.tooltip.visible"),
            ui.Text{text=ui.bind("bead.desktop.tray.tooltip"),height=24,width="100%",font_size=13,color="onSurface"},
        },
        ui.Repeat{id="menu",model=ui.bind("bead.desktop.menu"),key="id",render=function(item)
            return kit.Button{id="entry",text=item.label,height=30,padding=4,font_size=13,on_activate=ui.action("services.menu",item.id)}
        end},
    }}
            end
            local show_text=display~="icon"
            local glyph="desktop_windows"
            local show_icon=true
            local icon_width=show_icon and t.icon_width or 0
            local gap=show_icon and show_text and t.title_gap or 0
            local title_text=show_text and (ui.resolve_for_check(title) or "") or ""
            local app=active_app()
            if display=="app only" then title_text=app
            elseif display=="app + title" and app~="" then title_text=app.." · "..title_text end
            local centered=props.instance.host=="top_bar" and props.instance.slot.region=="centre" and not props.local_layout
            local available=centered and ((ui.resolve_for_check(ui.bind("bead.viewport.width")) or 1920)*.44) or max_width
            if props.available_width then available=math.min(available,props.available_width) end
            local capacity=math.max(0,math.min(max_width,available))
            local text_capacity=math.max(0,capacity-icon_width-gap)
            local natural_width=show_text and math.max(0,math.min(text_capacity,
                math.max(min_width-icon_width-gap,ui.text_width(title_text,"IBM Plex Sans",t.title_font)))) or 0
            -- Keep the allocation at its configured maximum reservation while the
            -- rendered child follows the natural title width smoothly.
            local animated_text_width=show_text and ui.animate("title.width."..props.instance.iid,natural_width,.18,"out-cubic") or 0
            -- A shrinking max width takes effect immediately even while the existing
            -- natural-width animation is still moving toward its new target.
            local text_width=math.max(0,math.min(animated_text_width,text_capacity))
            local shown=show_text and measure.ellipsize(title_text,text_width,"IBM Plex Sans",t.title_font) or ""
            local content_width=math.min(capacity,math.max(math.min(min_width,capacity),icon_width+gap+text_width))
            local action=behavior=="click" and ui.action("view.open","dashboard-today")
                or behavior=="hover" and ui.action("bar.title") or nil
            local kids={}
            if centered then
                local cue=math.min(t.title_cue_max,content_width+10)
                table.insert(kids,ui.Row{id="title-cue",position="absolute",left="28%",right="28%",top=7,height=36,justify="center",visible=ui.bind("bar.title.cue.visible"),
                    ui.Box{id="cue",width=cue+4,height=36,radius=18,fill=ui.alpha("primary",ui.bind("bar.title.cue.halo")),
                        ui.Box{id="body",position="absolute",left=2,top=2,width=cue,height=32,radius=16,fill=ui.alpha("primary",ui.bind("bar.title.cue.body")),border_width=1,border_color=ui.alpha("primary",ui.bind("bar.title.cue.border"))},
                        ui.Box{id="title-trace",position="absolute",left=7+(cue-10)*(1-dwell)/2,top=31,width=(cue-10)*dwell,height=t.title_trace_height,radius=1,fill=ui.alpha("primary",ui.bind("bar.title.cue.fade"))}}})
            end
            local content={id="title-content",width=content_width,height=t.title_height,gap=gap,align="center",justify="center",
                drag_payload=props.instance.iid,clip_children=true}
            if show_icon then
                table.insert(content,centered_text{id="title-icon",visible=ui.bind("bead.desktop.titlevisible"),text=glyph,width=icon_width,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,color="primary"})
            end
            if show_text then
                table.insert(content,centered_text{id="title",visible=ui.bind("bead.desktop.titlevisible"),text=shown,height=24,width=text_width,max_width=text_width,family="IBM Plex Sans",font_size=t.title_font,text_align="center",color="primary"})
            end
            local row={id=centered and "title-band" or "title-local",height=t.title_height,direction="row",gap=0,align="center",justify="center",
                edge_hover=behavior=="hover",activate_on_release=true,focus_on_pointer=false,on_activate=action,aria_label=ui.tr("bead.bar.active-window")}
            if centered then
                row.position="absolute"; row.left="28%"; row.right="28%"; row.top=(header-t.title_height)/2
                row.fill="transparent"; row.hover_fill="transparent"; row.focus_ring=false
            else row.width=content_width end
            table.insert(row,ui.Row(content))
            table.insert(row,kit.Tooltip{id="tray-tooltip",edge="top",bar_size=-8,left=0,width="100%",height=30,padding=3,visible=ui.bind("bead.desktop.tray.tooltip.visible"),
                ui.Text{text=ui.bind("bead.desktop.tray.tooltip"),height=24,width="100%",font_size=13,color="onSurface"}})
            table.insert(row,ui.Repeat{id="menu",model=ui.bind("bead.desktop.menu"),key="id",render=function(item)
                return kit.Button{id="entry",text=item.label,height=30,padding=4,font_size=13,on_activate=ui.action("services.menu",item.id)}
            end})
            table.insert(kids,ui.Button(row))
            kids.id="title-root"
            return ui.Group(kids)
    end}}}},panel={styles={dashboard={view="dashboard-today"}}}}
