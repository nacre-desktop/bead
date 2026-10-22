-- SPDX-License-Identifier: GPL-3.0-only
-- Date inputs come from the host; only the displayed month belongs to this view.
local months={"bead.common.month-january","bead.common.month-february","bead.common.month-march","bead.common.month-april","bead.common.month-may","bead.common.month-june","bead.common.month-july","bead.common.month-august","bead.common.month-september","bead.common.month-october","bead.common.month-november","bead.common.month-december"}
local weekdays={"bead.common.weekday-short-sun","bead.common.weekday-short-mon","bead.common.weekday-short-tue","bead.common.weekday-short-wed","bead.common.weekday-short-thu","bead.common.weekday-short-fri","bead.common.weekday-short-sat"}
local function daysInMonth(year,month)
    if month==2 then return year%4==0 and (year%100~=0 or year%400==0) and 29 or 28 end
    return ({31,28,31,30,31,30,31,31,30,31,30,31})[month]
end
local function valid(year,month)
    return year%1==0 and month%1==0 and year>=1 and year<=9999 and month>=1 and month<=12
end
local function displayed(s)
    local y,m=s["calendar.year"],s["calendar.month"]
    assert(valid(y,m),"Calendar needs a valid year and month")
    return y,m
end
local function shifted(y,m,delta)
    m+=delta
    y+=math.floor((m-1)/12)
    return y,(m-1)%12+1
end
return {
    stores={
        ["panel.open"]=true,["view.edge"]="top",
        ["calendar.ready"]=false,["calendar.initialized"]=false,["calendar.follow_today"]=false,["calendar.year"]=1970,["calendar.month"]=1,
        ["calendar.first_weekday"]=1,["calendar.today"]="",
    },
    actions={
        close=function(s) s["panel.open"]=false end,
        calendar_month=function(s,delta)
            if type(delta)~="number" or delta%1~=0 or math.abs(delta)>1200 then return end
            local y,m=displayed(s)
            y,m=shifted(y,m,delta)
            if valid(y,m) then
                s["calendar.follow_today"]=false
                s["calendar.year"],s["calendar.month"]=y,m
            end
        end,
    },
    computed={
        ["calendar.title"]=function(s)
            local y,m=displayed(s)
            return tr("bead.common.month-year",{month=tr(months[m]),year=y})
        end,
        ["calendar.weekdays"]=function(s)
            local first=s["calendar.first_weekday"]
            assert(first%1==0 and first>=0 and first<=6,"Calendar needs weekday0..6")
            local out={}
            for i=0,6 do
                local day=(first+i)%7
                out[i+1]={id="weekday-"..day,name=tr(weekdays[day+1])}
            end
            return out
        end,
        ["calendar.days"]=function(s)
            local y,m=displayed(s)
            local first=s["calendar.first_weekday"]
            assert(first%1==0 and first>=0 and first<=6,"Calendar needs weekday0..6")
            local adjusted=m<3 and y-1 or y
            local offsets={0,3,2,5,0,3,5,1,4,6,2,4}
            local weekday=(adjusted+math.floor(adjusted/4)-math.floor(adjusted/100)+math.floor(adjusted/400)+offsets[m]+1)%7
            local leading=(weekday-first)%7
            local out={}
            local today=s["calendar.today"]
            for i=0,41 do
                local day=i-leading+1
                local year,month=y,m
                if day<1 then
                    year,month=shifted(y,m,-1)
                    day+=daysInMonth(year,month)
                elseif day>daysInMonth(y,m) then
                    day-=daysInMonth(y,m)
                    year,month=shifted(y,m,1)
                end
                local key=string.format("%04d-%02d-%02d",year,month,day)
                local inRange=year>=1 and year<=9999
                out[i+1]={id=key,label=inRange and tostring(day) or "",in_month=year==y and month==m,
                    today=inRange and key==today}
            end
            return out
        end,
    },
}
