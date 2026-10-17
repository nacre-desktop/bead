-- SPDX-License-Identifier: GPL-3.0-only
return {stores={
    ["panel.open"]=true,["view.edge"]="top",
    ["dashboard.page"]="system",["dashboard.title"]="System",
    ["dashboard.preview"]=false,["dashboard.ready"]=false,["dashboard.stale"]=false,
    ["dashboard.heading"]="System unavailable",["dashboard.summary"]="Data is not available yet",
    ["dashboard.detail"]="",["dashboard.metric"]="—",
    ["dashboard.rows"]={},["dashboard.secondary"]={},
    ["dashboard.updates"]="Updates unavailable",["dashboard.updates_detail"]="",
},actions={close=function(s) s["panel.open"]=false end},computed={}}
