-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local measure=require("../measure")
local status_text=c.status_text
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local visual=require("../desktop-visuals")
local clock_font="IBM Plex Sans"
local display_font=visual.display_font
local time_max=math.max(t.clock_min_width,ui.text_width("23:59",clock_font,t.clock_font),
    ui.text_width("11:59 PM",clock_font,t.clock_font),ui.text_width("12:59 PM",clock_font,t.clock_font))
local seconds_max=math.max(ui.text_width("23:59:59",clock_font,t.clock_font),ui.text_width("12:59:59 PM",clock_font,t.clock_font))
local date_max=math.max(ui.text_width("9999-12-31",clock_font,t.detail_font),ui.text_width("8888-88-88",clock_font,t.detail_font),ui.text_width("0000-00-00",clock_font,t.detail_font))+kit.tokens.spacing.hairline
local date_content_max=time_max+t.item_gap+date_max
local function reserve_clock(content_width)
    return t.status_padding+t.status_count_padding+t.status_ink.calendar+t.item_gap+content_width
end
local function clock_with_seconds(time,seconds)
    local suffix=string.match(seconds,"^%d%d:%d%d:(%d%d)$")
    if not suffix then return time end
    local prefix,meridiem=string.match(time,"^(.-)(%s+[AP]M)$")
    if prefix then return prefix..":"..suffix..meridiem end
    return time..":"..suffix
end
local function natural_content_width(style)
    local time=ui.resolve_for_check(ui.bind("bar.clock.text")) or ""
    if style=="time" then
        local shown=measure.ellipsize(time,time_max,clock_font,t.clock_font)
        return math.max(t.clock_min_width,ui.text_width(shown,clock_font,t.clock_font))
    elseif style=="seconds" then
        local seconds=ui.resolve_for_check(ui.bind("bead.desktop.clock.seconds")) or ""
        local value=measure.ellipsize(clock_with_seconds(time,seconds),seconds_max,clock_font,t.clock_font)
        return ui.text_width(value,clock_font,t.clock_font)
    end
    local date=ui.resolve_for_check(ui.bind("bead.desktop.clock.date")) or ""
    local shown_time=measure.ellipsize(time,time_max,clock_font,t.clock_font)
    local shown_date=measure.ellipsize(date,date_max,clock_font,t.detail_font)
    local time_width=ui.text_width(shown_time,clock_font,t.clock_font)
    local date_width=ui.text_width(shown_date,clock_font,t.detail_font)
    return time_width+date_width+(time_width>0 and date_width>0 and t.item_gap or 0)
end
local function clock_content(style,instance_id,available_width)
    local time=ui.resolve_for_check(ui.bind("bar.clock.text")) or ""
    local time_capacity=math.min(time_max,available_width or time_max)
    local preview=type(instance_id)=="string" and string.sub(instance_id,1,8)=="preview-"
    if style=="time" then
        local shown=measure.ellipsize(time,time_capacity,clock_font,t.clock_font)
        local natural=math.min(time_capacity,math.max(math.min(t.clock_min_width,time_capacity),
            ui.text_width(shown,clock_font,t.clock_font)))
        ui.animate("clock-date-width:"..tostring(instance_id or "clock"),natural,0,"out-cubic")
        return status_text{id="time",text=shown,width=natural,max_width=time_capacity,shrink=0,font_size=t.clock_font,color="tertiary"}
    elseif style=="seconds" then
        local seconds=ui.resolve_for_check(ui.bind("bead.desktop.clock.seconds")) or ""
        local capacity=math.min(seconds_max,available_width or seconds_max)
        local value=clock_with_seconds(time,seconds)
        local natural=ui.text_width(measure.ellipsize(value,capacity,clock_font,t.clock_font),clock_font,t.clock_font)
        local width=preview and natural or math.min(capacity,
            ui.animate("clock-seconds-width:"..tostring(instance_id or "clock"),natural,.18,"out-cubic"))
        local shown=measure.ellipsize(value,width,clock_font,t.clock_font)
        return status_text{id="time-seconds",text=shown,width=width,max_width=capacity,shrink=0,font_size=t.clock_font,color="tertiary"}
    end
    local date=ui.resolve_for_check(ui.bind("bead.desktop.clock.date")) or ""
    local capacity=math.min(date_content_max,available_width or date_content_max)
    local time_natural=ui.text_width(measure.ellipsize(time,time_max,clock_font,t.clock_font),clock_font,t.clock_font)
    local date_natural=ui.text_width(measure.ellipsize(date,date_max,clock_font,t.detail_font),clock_font,t.detail_font)
    local target_time=math.min(time_natural,capacity)
    local target_date=math.min(date_natural,math.max(0,capacity-target_time-(date_natural>0 and t.item_gap or 0)))
    local target=target_time+target_date+(target_date>0 and target_time>0 and t.item_gap or 0)
    local width=preview and target or math.min(capacity,
        ui.animate("clock-date-width:"..tostring(instance_id or "clock"),target,.18,"out-cubic"))
    local time_width=math.min(time_natural,math.max(0,width-(width>time_natural and t.item_gap or 0)))
    local date_width=math.min(date_natural,math.max(0,width-time_width-(time_width>0 and t.item_gap or 0)))
    local gap=date_width>0 and time_width>0 and t.item_gap or 0
    local date_node=status_text{id="date",text=measure.ellipsize(date,date_width,clock_font,t.detail_font),width=date_width,max_width=date_width,shrink=0,font_size=t.detail_font,color="secondary"}
    local time_node=status_text{id="time",text=measure.ellipsize(time,time_width,clock_font,t.clock_font),width=time_width,max_width=time_width,shrink=0,font_size=t.clock_font,color="tertiary"}
    return ui.Row{id="time-date",width=width,height=t.status_height,gap=gap,align="center",time_node,date_node}
end
local function bar(props,settings,_data)
    local style=props.form_style
    local inner_capacity=props.available_width and math.max(0,props.available_width-t.status_padding-t.status_count_padding
        -t.status_ink.calendar-t.item_gap) or nil
    local content=clock_content(style,props.instance and props.instance.iid,inner_capacity)
    return (popouts and kit.Button or ui.Row){id="clock",height=t.status_height,direction=popouts and "row" or nil,gap=t.item_gap,padding=0,padding_left=t.status_padding,padding_right=t.status_count_padding,align="center",fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-calendar") or nil,on_activate=popouts and ui.action("view.open","calendar") or nil,
        status_glyph("calendar","calendar_month","calendar","tertiary"),content,
    }
end
local function desktop(props,_settings,_data)
    local style=props.form_style or "digital"
    local w,h=visual.slot(props.instance,420,190)
    local action=popouts and ui.action("view.open","calendar") or nil
    local time=ui.bind("bar.clock.text")
    if style=="seconds" then
        time=clock_with_seconds(ui.resolve_for_check(ui.bind("bar.clock.text")) or "",
            ui.resolve_for_check(ui.bind("bead.desktop.clock.seconds")) or "")
    end
    local visible_time=tostring(ui.resolve_for_check(time) or "")
    local iso=ui.resolve_for_check(ui.bind("bead.desktop.clock.date")) or ""
    local year,month,day=string.match(iso,"^(%d%d%d%d)%-(%d%d)%-(%d%d)$")
    local weekday=""
    if year then
        year,month,day=tonumber(year),tonumber(month),tonumber(day)
        if month<3 then month+=12; year-=1 end
        local zeller=(day+math.floor(13*(month+1)/5)+year+math.floor(year/4)-math.floor(year/100)+math.floor(year/400))%7
        weekday=({[0]="Saturday",[1]="Sunday",[2]="Monday",[3]="Tuesday",[4]="Wednesday",[5]="Thursday",[6]="Friday"})[zeller]
    end
    local _,month_text,day_text=string.match(iso,"^(%d%d%d%d)%-(%d%d)%-(%d%d)$")
    local months={"January","February","March","April","May","June","July","August","September","October","November","December"}
    local month=tonumber(month_text)
    local date_line=weekday~="" and month and (weekday..", "..tonumber(day_text).." "..months[month]) or iso
    if style=="analog" then
        local seconds=ui.resolve_for_check(ui.bind("bead.desktop.clock.seconds")) or "00:00:00"
        local hours,minutes=string.match(visible_time,"^(%d%d?):(%d%d)")
        hours,minutes=tonumber(hours) or 0,tonumber(minutes) or 0
        local second=tonumber(string.match(seconds,"%d%d$")) or 0
        local clock_seconds=hours*3600+minutes*60+second
        local second_angle=ui.animate("wall-clock-second",clock_seconds*6,1,"linear")
        local minute_angle=ui.animate("wall-clock-minute",clock_seconds*.1,1,"linear")
        local hour_angle=ui.animate("wall-clock-hour",clock_seconds/120,1,"linear")
        local dial_left,dial_size=8,h-16
        local function hand(id,degrees,length,width,color)
            local radians=math.rad(degrees-90)
            return ui.Polyline{id=id,position="absolute",left=dial_left,top=dial_left,width=dial_size,height=dial_size,
                points={{.5,.5},{.5+math.cos(radians)*length,.5+math.sin(radians)*length}},
                stroke_width=width,round_caps=true,color=color}
        end
        local ticks={}
        for index=0,59 do
            local angle=math.rad(index*6-90)
            local major=index%5==0
            local outer=major and .46 or .465
            local inner=major and .39 or .435
            table.insert(ticks,ui.Polyline{id="tick-"..tostring(index),position="absolute",left=dial_left,top=dial_left,
                width=dial_size,height=dial_size,points={{.5+math.cos(angle)*outer,.5+math.sin(angle)*outer},
                    {.5+math.cos(angle)*inner,.5+math.sin(angle)*inner}},stroke_width=major and 4 or 2,
                round_caps=true,color=major and ui.alpha("onSurface",.85) or ui.alpha("onSurface",.48)})
        end
        return ui.Box{id="clock-analog",width=w,height=h,fill=visual.glass_fill,radius=h/2,
            border_width=1,border_color=visual.glass_border,on_activate=action,
            table.unpack(ticks),
            hand("hour-hand",hour_angle,.26,7,"onSurface"),
            hand("minute-hand",minute_angle,.37,4.5,"onSurface"),
            hand("second-hand",second_angle,.42,2,"primary"),
            ui.Box{id="hand-pin",position="absolute",left=h/2-5,top=h/2-5,width=10,height=10,radius=5,fill="primary"},
            ui.Column{id="dial-caption",position="absolute",left=h+kit.tokens.spacing.lg,top=0,width=w-h-kit.tokens.spacing.lg,height=h,justify="center",gap=kit.tokens.spacing.sm,
                ui.Text{id="weekday",text=date_line,width="fill",height=28,font_size=kit.tokens.text.title*4/3,color="primary"},
                ui.Text{id="digital-time",text=time,width="fill",height=56,font_size=kit.tokens.text.headline*2,color="onSurface"}}}
    elseif style=="minimal" then
        local hour,minute=string.match(visible_time,"^(%d%d?):(%d%d)")
        return ui.Column{id="clock-minimal",width=w,height=h,justify="center",gap=kit.tokens.spacing.xs,on_activate=action,
            visual.display("hour",hour or visible_time,"fill",h*.38,math.min(110,h*.42),"onSurface"),
            visual.display("minute",minute or "","fill",h*.38,math.min(110,h*.42),"primary"),
            ui.Text{id="date",text=weekday,width="fill",height=24,font_size=kit.tokens.text.body*4/3,color="secondary"}}
    end
    return ui.Box{id="clock-desktop",width=w,height=h,padding=kit.tokens.spacing.lg,radius=kit.tokens.radii.large,fill=visual.glass_fill,border_width=1,border_color=visual.glass_border,on_activate=action,
        ui.Column{id="clock-content",width="fill",height="fill",justify="center",gap=kit.tokens.spacing.xs,
            ui.Text{id="weekday",text=weekday,width="fill",height=26,font_size=kit.tokens.text.body*4/3,color="primary"},
            visual.display("time",time,"fill",h*.52,math.min(120,h*.6),"onSurface"),
            ui.Text{id="date",text=iso,width="fill",height=24,font_size=kit.tokens.text.body*4/3,color="onSurfaceVariant"}}}
end
local function bar_style(title,style,preferred,requires)
    local descriptor={title=title,min_span=1,max_span=60,preferred_span=preferred,requires=requires,render=function(props,settings,data)
        props.form_style=style
        return bar(props,settings,data)
    end}
    descriptor.reserve_width=function(_) return reserve_clock(natural_content_width(style)) end
    return descriptor
end
local function preferred_span(width)
    return math.ceil(reserve_clock(width)/32)
end
local function desktop_style(title,style,preferred,requires,preferred_height)
    return {title=title,min_span=1,max_span=1200,preferred_span=preferred,preferred_height=preferred_height or 190,requires=requires,render=function(props,settings,data)
        props.form_style=style
        return desktop(props,settings,data)
    end}
end
return {id="clock",title="Clock",icon="schedule",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="time",styles={
        time=bar_style("Time","time",preferred_span(time_max),nil),["time+date"]=bar_style("Time + date","time+date",preferred_span(date_content_max),{"bead.desktop.clock.date"}),
        seconds=bar_style("Seconds","seconds",preferred_span(seconds_max),{"bead.desktop.clock.seconds"}),
    }},desktop={default_style="digital",styles={
        time={title="Stored time",min_span=1,max_span=1200,preferred_span=220,preferred_height=92,hidden=true,render=function(p,s,d) p.form_style="digital"; return desktop(p,s,d) end},
        ["time+date"]={title="Stored time and date",min_span=1,max_span=1200,preferred_span=360,preferred_height=92,hidden=true,requires={"bead.desktop.clock.date"},render=function(p,s,d) p.form_style="digital"; return desktop(p,s,d) end},
        seconds={title="Stored seconds",min_span=1,max_span=1200,preferred_span=240,preferred_height=92,hidden=true,requires={"bead.desktop.clock.seconds"},render=function(p,s,d) p.form_style="seconds"; return desktop(p,s,d) end},
        digital=desktop_style("A · Digital","digital",420,{"bead.desktop.clock.date"}),
        analog=desktop_style("B · Analog dial","analog",420,{"bead.desktop.clock.seconds","bead.desktop.clock.date"},300),
        minimal=desktop_style("C · Minimal type","minimal",420,{"bead.desktop.clock.date"}),
    }},panel={styles={calendar={view="calendar"}}}}
}
