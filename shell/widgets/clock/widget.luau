-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local status_text=c.status_text
local popouts=c.popouts
local t=c.t
local status_glyph=c.status_glyph
local function fit_text(text,width,font,size)
    if width<=0 then return "" end
    if ui.text_width(text,font,size)<=width then return text end
    local points={}
    for _,point in utf8.codes(text) do points[#points+1]=utf8.char(point) end
    local low,high,best=0,#points,0
    while low<=high do
        local middle=math.floor((low+high)/2)
        local candidate=table.concat(points,"",1,middle).."…"
        if ui.text_width(candidate,font,size)<=width then best=middle; low=middle+1
        else high=middle-1 end
    end
    return table.concat(points,"",1,best).."…"
end
local clock_font="IBM Plex Sans"
local time_max=math.max(t.clock_min_width,ui.text_width("23:59",clock_font,t.clock_font),ui.text_width("12:59 PM",clock_font,t.clock_font))
local seconds_max=math.max(ui.text_width("23:59:59",clock_font,t.clock_font),ui.text_width("12:59:59 PM",clock_font,t.clock_font))
local date_max=ui.text_width("9999-12-31",clock_font,t.detail_font)
local date_content_max=time_max+t.item_gap+date_max
local seconds_content_max=seconds_max
local function clock_with_seconds(time,seconds)
    local suffix=string.match(seconds,"^%d%d:%d%d:(%d%d)$")
    if not suffix then return time end
    local prefix,meridiem=string.match(time,"^(.-)(%s+[AP]M)$")
    if prefix then return prefix..":"..suffix..meridiem end
    return time..":"..suffix
end
local function clock_content(style)
    local time=ui.resolve_for_check(ui.bind("bar.clock.text")) or ""
    if style=="time" then
        local shown=fit_text(time,time_max,clock_font,t.clock_font)
        return status_text{id="time",text=shown,width=math.max(t.clock_min_width,ui.text_width(shown,clock_font,t.clock_font)),max_width=time_max,shrink=0,font_size=t.clock_font,color="tertiary"}
    elseif style=="seconds" then
        local seconds=ui.resolve_for_check(ui.bind("bead.desktop.clock.seconds")) or ""
        local shown=fit_text(clock_with_seconds(time,seconds),seconds_max,clock_font,t.clock_font)
        return status_text{id="time-seconds",text=shown,width=ui.text_width(shown,clock_font,t.clock_font),max_width=seconds_max,shrink=0,font_size=t.clock_font,color="tertiary"}
    end
    local date=ui.resolve_for_check(ui.bind("bead.desktop.clock.date")) or ""
    local date_node=status_text{id="date",text=fit_text(date,date_max,clock_font,t.detail_font),width=date_max,max_width=date_max,shrink=0,font_size=t.detail_font,color="secondary"}
    local time_node=status_text{id="time",text=fit_text(time,time_max,clock_font,t.clock_font),width=time_max,max_width=time_max,shrink=0,font_size=t.clock_font,color="tertiary"}
    return ui.Row{id="time-date",height=t.status_height,gap=t.item_gap,align="center",time_node,date_node}
end
local function bar(props,settings,_data)
    local style=props.form_style
    local content=clock_content(style)
    local value=ui.resolve_for_check(ui.bind("bar.clock.text")) or ""
    local natural
    if style=="time" then
        natural=math.max(t.clock_min_width,ui.text_width(fit_text(value,time_max,clock_font,t.clock_font),clock_font,t.clock_font))
    elseif style=="seconds" then
        natural=seconds_content_max
    else
        natural=date_content_max
    end
    content.props.width=natural
    return (popouts and kit.Button or ui.Row){id="clock",height=t.status_height,direction=popouts and "row" or nil,gap=t.item_gap,padding=0,padding_left=t.status_padding,padding_right=t.status_count_padding,align="center",fill=popouts and "transparent" or nil,aria_label=popouts and ui.tr("bead.bar.open-calendar") or nil,on_activate=popouts and ui.action("view.open","calendar") or nil,
        status_glyph("calendar","calendar_month","calendar","tertiary"),content,
    }
end
local function desktop(props,_settings,_data)
    local slot=props.instance.slot or {w=320,h=92}
    return ui.Box{id="clock-desktop",width=slot.w,height=slot.h,padding=12,radius=kit.tokens.radii.medium,fill="surfaceContainer",on_activate=ui.action("view.open","calendar"),
        ui.Column{id="clock-content",width="fill",height="fill",align="center",justify="center",gap=6,
            status_glyph("calendar","calendar_month","calendar","tertiary"),
            clock_content(props.form_style),
        }}
end
local function bar_style(title,style,preferred,requires)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred,requires=requires,render=function(props,settings,data)
        props.form_style=style
        return bar(props,settings,data)
    end}
end
local function desktop_style(title,style,preferred,requires)
    return {title=title,min_span=1,max_span=1200,preferred_span=preferred,requires=requires,render=function(props,settings,data)
        props.form_style=style
        return desktop(props,settings,data)
    end}
end
return {id="clock",title="Clock",icon="widgets",category="bar",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="time",styles={
        time=bar_style("Time","time",1),["time+date"]=bar_style("Time + date","time+date",6,{"bead.desktop.clock.date"}),
        seconds=bar_style("Seconds","seconds",2,{"bead.desktop.clock.seconds"}),
    }},desktop={default_style="time",styles={
        time=desktop_style("Time","time",220),["time+date"]=desktop_style("Time + date","time+date",360,{"bead.desktop.clock.date"}),
        seconds=desktop_style("Seconds","seconds",240,{"bead.desktop.clock.seconds"}),
    }},panel={styles={calendar={view="calendar"}}}}
}
