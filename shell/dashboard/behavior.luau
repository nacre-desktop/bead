-- SPDX-License-Identifier: GPL-3.0-only
return {stores={
    ["panel.open"]=true,["view.edge"]="top",
    ["dashboard.weather.metrics"]={},["dashboard.weather.sun_times"]="—",["dashboard.weather.photos_enabled"]=true,["dashboard.weather.photo"]=false,["dashboard.weather.photo_w"]=0,["dashboard.weather.photo_h"]=0,["dashboard.weather.photo_author"]="",["dashboard.weather.photo_license"]="",["dashboard.weather.photo_link"]=false,["dashboard.weather.city"]="",["dashboard.weather.code"]=-1,["dashboard.weather.day"]=true,["dashboard.weather.scroll"]=0,
    ["dashboard.page"]="system",["dashboard.title"]=tr("bead.dashboard.page-system"),
    ["dashboard.pearl.x"]=0,["dashboard.pearl.target"]=0,["dashboard.pearl.moving"]=false,["dashboard.pearl.duration"]=0.18,["dashboard.pearl.curve"]="out-cubic",
    ["dashboard.preview"]=false,["dashboard.ready"]=false,["dashboard.stale"]=false,
    ["dashboard.heading"]=tr("bead.dashboard.page-unavailable",{page=tr("bead.dashboard.page-system")}),["dashboard.summary"]=tr("bead.dashboard.data-pending"),
    ["dashboard.detail"]="",["dashboard.metric"]="—",
    ["dashboard.rows"]={},["dashboard.secondary"]={},["dashboard.host"]={},
    ["dashboard.updates"]=tr("bead.dashboard.updates-unavailable"),["dashboard.updates_detail"]="",
    ["dashboard.tabs"]={{id="today",title=tr("bead.dashboard.page-today"),icon="dashboard"},{id="weather",title=tr("bead.dashboard.page-weather"),icon="partly_cloudy_day"},{id="media",title=tr("bead.dashboard.page-media"),icon="music_note"},{id="system",title=tr("bead.dashboard.page-system"),icon="monitoring"}},
    ["dashboard.control"]=false,["dashboard.busy"]=false,["dashboard.message"]="",["dashboard.confirm"]=false,["dashboard.dnd"]=false,
        ["dashboard.artist"]="",["dashboard.album"]="",["dashboard.player_identity"]="",["dashboard.track"]=tr("bead.dashboard.playback-unavailable"),["dashboard.track_detail"]=tr("bead.dashboard.playback-unavailable-detail"),
    ["dashboard.can_previous"]=false,["dashboard.can_toggle"]=false,["dashboard.can_next"]=false,
    ["dashboard.gauges"]={{id="cpu",label=tr("bead.dashboard.gauge-cpu"),icon="developer_board",value="—",detail=tr("bead.dashboard.gauge-unavailable")},{id="memory",label=tr("bead.dashboard.gauge-memory"),icon="memory",value="—",detail=tr("bead.dashboard.gauge-unavailable")},{id="gpu",label=tr("bead.dashboard.gauge-gpu"),icon="view_in_ar",value="—",detail=tr("bead.dashboard.gauge-unavailable")}},
    ["dashboard.can_seek"]=false,["dashboard.art"]=false,
    ["dashboard.net_down"]={},["dashboard.net_up"]={},["dashboard.net_rate"]="—",["dashboard.net_up_rate"]="—",["dashboard.net_peak"]="",
    ["dashboard.customize"]=false,["dashboard.state"]="unavailable",["dashboard.progress"]=0,["dashboard.minutes"]=25,
},actions={close=function(s) s["panel.open"]=false end,
    ["dashboard.select"]=function(s,id)
        local tabs=s["dashboard.tabs"]
        local count=math.max(1,#tabs)
        local width=936/count
        if not s["dashboard.pearl.moving"] then
            for index,tab in ipairs(tabs) do
                if tab.id==s["dashboard.page"] then
                    s["dashboard.pearl.x"]=(index-1)*width+(width-12)/2
                    break
                end
            end
        end
        for index,tab in ipairs(tabs) do
            if tab.id==id then
                s["dashboard.pearl.target"]=(index-1)*width+(width-12)/2
                s["dashboard.pearl.moving"]=true
                return
            end
        end
    end,
    focus_minutes=function(s,m) if m==15 or m==25 or m==50 then s["dashboard.minutes"]=m end end},computed={}}
