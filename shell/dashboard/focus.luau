-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    local state=ui.resolve_for_check(ui.bind("dashboard.state"))
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local minutes=ui.resolve_for_check(ui.bind("dashboard.minutes"))
    local progress=ui.resolve_for_check(ui.bind("dashboard.progress"))
    local live=state=="running" or state=="paused"
    local buttons={}
    if live then
        local op=state=="running" and "focus.pause" or "focus.resume"
        table.insert(buttons,C.button("main",state=="running" and "Pause" or "Resume",control and ui.action("dashboard.do",op) or nil))
        table.insert(buttons,C.button("stop","Stop",control and ui.action("dashboard.do","focus.stop") or nil))
    else
        for _,m in ipairs({15,25,50}) do
            table.insert(buttons,C.button("min"..m,m.." min",ui.action("focus_minutes",m),110,minutes==m))
        end
        table.insert(buttons,C.button("start","Start focus",control and ui.action("dashboard.do","focus.start:"..tostring(minutes)) or nil))
    end
    return ui.Column{id="focus",width=940,height=562,padding=22,gap=18,radius=20,fill="surfaceContainer",align="center",justify="center",
        ui.Text{id="status",text=ui.bind("dashboard.heading"),width=896,height=28,font_size=20,text_align="center",color="onSurfaceVariant"},
        ui.Text{id="timer",text=live and ui.bind("dashboard.metric") or string.format("%02d:00",minutes),width=896,height=96,font_size=80,text_align="center",color="primary"},
        ui.Box{id="track",width=600,height=6,radius=3,fill=ui.alpha("primary",0.16),
            progress>0 and ui.Box{id="elapsed",width=math.max(1,math.floor(600*progress)),height=6,radius=3,fill="primary"} or nil},
        ui.Row{id="controls",width=896,height=40,gap=12,justify="center",table.unpack(buttons)},
        ui.Text{id="note",text=ui.bind("dashboard.summary"),width=896,height=24,font_size=14,text_align="center",color="onSurfaceVariant"},
    }
end
