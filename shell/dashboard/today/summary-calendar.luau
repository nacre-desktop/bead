-- SPDX-License-Identifier: GPL-3.0-only
-- NacreCalendarCard geometry, sized to the Today summary column.
local ui=require("bead.ui")
local kit=require("bead.kit")
local function monthButton(id,icon,label,delta,left)
    return kit.IconButton{id=id,text=icon,aria_label=label,left=left,top=8,position="absolute",
        width=30,height=30,padding=0,radius=15,fill="transparent",hover_fill=ui.alpha("onSurface",.08),
        font_size=kit.tokens.icons.medium*4/3,text_align="center",on_activate=ui.action("calendar_month",delta)}
end
return function(width,height)
    width,height=width or 291.4,height or 288
    local titleSize=math.min(14,math.max(11,width/40))*4/3
    local weekdaySize=10*4/3
    local daySize=titleSize
    local cellWidth=(width-24)/7
    local dayHeight=math.max(24,math.min(54,(height-92)/6))
    return ui.Box{id="calendar",width=width,height=height,
        ui.Text{id="heading",position="absolute",left=44,top=12,width=width-88,height=24,
            text=ui.bind("calendar.title"),font_size=titleSize,text_align="center"},
        monthButton("previous","chevron_left",ui.tr("bead.panel.calendar-previous"),-1,8),
        monthButton("next","chevron_right",ui.tr("bead.panel.calendar-next"),1,width-38),
        ui.Grid{id="weekdays",position="absolute",left=12,top=52,width=width-24,columns=7,row_height=28,
            ui.Repeat{id="names",model=ui.bind("calendar.weekdays"),key="id",render=function(item)
                return ui.Box{id="weekday",width=cellWidth,height=28,
                    ui.Text{id="label",position="absolute",top=5,width=cellWidth,height=18,text=item.name,
                        font_size=weekdaySize,text_align="center",color="onSurfaceVariant"}}
            end}},
        ui.Grid{id="days",position="absolute",left=12,top=80,width=width-24,columns=7,row_height=dayHeight,
            ui.Repeat{id="dates",model=ui.bind("calendar.days"),key="id",render=function(item)
                return ui.Box{id="cell",width=cellWidth,height=dayHeight,
                    ui.Box{id="today",position="absolute",left=math.max(0,(cellWidth-30)/2),top=1,width=30,height=30,
                        radius=15,fill="primary",visible=item.today},
                    ui.Text{id="label",position="absolute",top=math.floor((dayHeight-20)/2),width=cellWidth,height=20,text=item.label,
                        font_size=daySize,text_align="center",color=item.today and "onPrimary" or item.in_month and "onSurface" or "onSurfaceVariant"}}
            end}},
    }
end
