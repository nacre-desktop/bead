-- SPDX-License-Identifier: GPL-3.0-only
return {stores={
    ["panel.open"]=true,["view.edge"]="top",
    ["dashboard.page"]="system",["dashboard.title"]="System",
    ["dashboard.preview"]=false,["dashboard.ready"]=false,["dashboard.stale"]=false,
    ["dashboard.heading"]="System unavailable",["dashboard.summary"]="Data is not available yet",
    ["dashboard.detail"]="",["dashboard.metric"]="—",
    ["dashboard.rows"]={},["dashboard.secondary"]={},
    ["dashboard.updates"]="Updates unavailable",["dashboard.updates_detail"]="",
    ["dashboard.tabs"]={{id="today",title="Today",icon="dashboard"},{id="weather",title="Weather",icon="partly_cloudy_day"},{id="media",title="Media",icon="music_note"},{id="system",title="System",icon="monitoring"}},
    ["dashboard.control"]=false,["dashboard.busy"]=false,["dashboard.message"]="",["dashboard.confirm"]=false,
        ["dashboard.track"]="Playback unavailable",["dashboard.track_detail"]="Playback details and controls are not available yet",
    ["dashboard.can_previous"]=false,["dashboard.can_toggle"]=false,["dashboard.can_next"]=false,
    ["dashboard.gauges"]={{id="cpu",label="CPU",icon="developer_board",value="—",detail="Metric unavailable"},{id="memory",label="Memory",icon="memory",value="—",detail="Metric unavailable"},{id="gpu",label="GPU",icon="view_in_ar",value="—",detail="Metric unavailable"},{id="disk",label="Disk",icon="hard_drive",value="—",detail="See drives below"}},
    ["dashboard.customize"]=false,["dashboard.state"]="unavailable",["dashboard.progress"]=0,["dashboard.minutes"]=25,
},actions={close=function(s) s["panel.open"]=false end,
    focus_minutes=function(s,m) if m==15 or m==25 or m==50 then s["dashboard.minutes"]=m end end},computed={}}
