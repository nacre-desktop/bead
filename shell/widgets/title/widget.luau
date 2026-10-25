-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local centered_text=c.centered_text
local header=c.header
local title=c.title
local title_width=c.title_width
local cue_extent=c.cue_extent
local dwell=c.dwell
return {id="title",title="Title",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            return ui.Group{ui.Row{id="title-cue",position="absolute",left="28%",right="28%",top=7,height=36,justify="center",visible=ui.bind("bar.title.cue.visible"),
        ui.Box{id="cue",width=cue_extent+4,height=36,radius=18,fill=ui.alpha("primary",ui.bind("bar.title.cue.halo")),
            ui.Box{id="body",position="absolute",left=2,top=2,width=cue_extent,height=32,radius=16,fill=ui.alpha("primary",ui.bind("bar.title.cue.body")),border_width=1,border_color=ui.alpha("primary",ui.bind("bar.title.cue.border"))},
            ui.Box{id="title-trace",position="absolute",left=7+(cue_extent-10)*(1-dwell)/2,top=31,width=(cue_extent-10)*dwell,height=t.title_trace_height,radius=1,fill=ui.alpha("primary",ui.bind("bar.title.cue.fade"))},
        },
    },
    ui.Row{id="title-band",position="absolute",left="28%",right="28%",top=(header-t.title_height)/2,height=t.title_height,justify="center",align="center",gap=t.title_gap,fill="transparent",hover_fill="transparent",focus_ring=false,edge_hover=true,aria_label=ui.tr("bead.bar.active-window"),on_activate=ui.action("bar.title"),
        centered_text{id="title-icon",visible=ui.bind("bead.desktop.titlevisible"),text="desktop_windows",width=24,height=24,shrink=0,family="Material Symbols Rounded",font_size=20,color="primary"},
        centered_text{id="title",visible=ui.bind("bead.desktop.titlevisible"),text=title,height=24,width=title_width,max_width=t.title_max_width,family="IBM Plex Sans",font_size=t.title_font,text_align="center",color="primary"},
        kit.Tooltip{id="tray-tooltip",edge="top",bar_size=-8,left=0,width="100%",height=30,padding=3,visible=ui.bind("bead.desktop.tray.tooltip.visible"),
            ui.Text{text=ui.bind("bead.desktop.tray.tooltip"),height=24,width="100%",font_size=13,color="onSurface"},
        },
        ui.Repeat{id="menu",model=ui.bind("bead.desktop.menu"),key="id",render=function(item)
            return kit.Button{id="entry",text=item.label,height=30,padding=4,font_size=13,on_activate=ui.action("services.menu",item.id)}
        end},
    }}
    end}}}
