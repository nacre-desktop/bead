-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local measure=require("../measure")
local status_text=c.status_text
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local clock_font="IBM Plex Sans"
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
local function clock_content(style,instance_id,available_width)
    local time=ui.resolve_for_check(ui.bind("bar.clock.text")) or ""
    local time_capacity=math.min(time_max,available_width or time_max)
    if style=="time" then
        local shown=measure.ellipsize(time,time_capacity,clock_font,t.clock_font)
        local natural=math.min(time_capacity,math.max(math.min(t.clock_min_width,time_capacity),
            ui.text_width(shown,clock_font,t.clock_font)))
        local width=ui.animate("clock-time-width:"..tostring(instance_id or "clock"),natural,.18,"out-cubic")
        return status_text{id="time",text=shown,width=math.min(time_capacity,width),max_width=time_capacity,shrink=0,font_size=t.clock_font,color="tertiary"}
    elseif style=="seconds" then
        local seconds=ui.resolve_for_check(ui.bind("bead.desktop.clock.seconds")) or ""
        local capacity=math.min(seconds_max,available_width or seconds_max)
        local value=clock_with_seconds(time,seconds)
        local natural=ui.text_width(measure.ellipsize(value,capacity,clock_font,t.clock_font),clock_font,t.clock_font)
        local width=math.min(capacity,ui.animate("clock-seconds-width:"..tostring(instance_id or "clock"),natural,.18,"out-cubic"))
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
    local width=math.min(capacity,ui.animate("clock-date-width:"..tostring(instance_id or "clock"),target,.18,"out-cubic"))
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
    local slot=props.instance.slot or {w=320,h=92}
    return ui.Box{id="clock-desktop",width=slot.w,height=slot.h,padding=kit.tokens.spacing.md,radius=kit.tokens.radii.medium,fill="surfaceContainer",on_activate=popouts and ui.action("view.open","calendar") or nil,
        ui.Column{id="clock-content",width="fill",height="fill",align="center",justify="center",gap=kit.tokens.spacing.md/2,
            status_glyph("calendar","calendar_month","calendar","tertiary"),
            clock_content(props.form_style,props.instance and props.instance.iid),
        }}
end
local function bar_style(title,style,preferred,requires,natural_width)
    local descriptor={title=title,min_span=1,max_span=60,preferred_span=preferred,requires=requires,render=function(props,settings,data)
        props.form_style=style
        return bar(props,settings,data)
    end}
    descriptor.reserve_width=function(_) return reserve_clock(natural_width) end
    return descriptor
end
local function preferred_span(width)
    return math.ceil(reserve_clock(width)/32)
end
local function desktop_style(title,style,preferred,requires)
    return {title=title,min_span=1,max_span=1200,preferred_span=preferred,requires=requires,render=function(props,settings,data)
        props.form_style=style
        return desktop(props,settings,data)
    end}
end
return {id="clock",title="Clock",icon="schedule",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="time",styles={
        time=bar_style("Time","time",preferred_span(time_max),nil,time_max),["time+date"]=bar_style("Time + date","time+date",preferred_span(date_content_max),{"bead.desktop.clock.date"},date_content_max),
        seconds=bar_style("Seconds","seconds",preferred_span(seconds_max),{"bead.desktop.clock.seconds"},seconds_max),
    }},desktop={default_style="time",styles={
        time=desktop_style("Time","time",220),["time+date"]=desktop_style("Time + date","time+date",360,{"bead.desktop.clock.date"}),
        seconds=desktop_style("Seconds","seconds",240,{"bead.desktop.clock.seconds"}),
    }},panel={styles={calendar={view="calendar"}}}}
}
