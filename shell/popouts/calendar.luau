-- SPDX-License-Identifier: GPL-3.0-only
-- NacreCalendarPopup dimensions and type scale; dates are view data, not actions.
local ui=require("bead.ui")
local kit=require("bead.kit")
local function monthButton(id,icon,label,delta,left)
    return kit.IconButton{id=id,text=icon,aria_label=label,left=left,top=0,
        position="absolute",width=34,height=34,padding=0,radius=17,
        fill="transparent",hover_fill=ui.tint("surface","onSurface",.08),
        font_size=kit.tokens.icons.medium*4/3,text_align="center",
        on_activate=ui.action("calendar_month",delta)}
end
return function()
    return ui.Box{id="calendar",width=280,height=282,
        ui.Text{id="heading",position="absolute",left=38,top=7,width=204,height=20,
            text=ui.bind("calendar.title"),font_size=kit.tokens.text.body*4/3,text_align="center"},
        monthButton("previous","chevron_left",ui.tr("bead.panel.calendar-previous"),-1,0),
        monthButton("next","chevron_right",ui.tr("bead.panel.calendar-next"),1,246),
        ui.Grid{id="weekdays",position="absolute",left=0,top=40,width=280,columns=7,row_height=28,
            ui.Repeat{id="names",model=ui.bind("calendar.weekdays"),key="id",render=function(item)
                return ui.Box{id="cell",width=40,height=28,
                    ui.Text{id="label",position="absolute",top=5,width=40,height=18,text=item.name,
                        font_size=kit.tokens.text.caption*4/3,text_align="center",color="onSurfaceVariant"}}
            end}},
        ui.Grid{id="days",position="absolute",left=0,top=68,width=280,columns=7,row_height=34,
            ui.Repeat{id="dates",model=ui.bind("calendar.days"),key="id",render=function(item)
                return ui.Box{id="cell",width=40,height=34,
                    ui.Box{id="today",position="absolute",left=5,top=2,width=30,height=30,radius=15,
                        fill="primary",visible=item.today},
                    ui.Text{id="label",position="absolute",top=7,width=40,height=20,text=item.label,
                        font_size=(kit.tokens.text.body+kit.tokens.text.caption)*2/3,text_align="center",
                        color=item.today and "onPrimary" or item.in_month and "onSurface" or "onSurfaceVariant"}}
            end}},
    }
end
