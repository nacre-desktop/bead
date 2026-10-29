-- SPDX-License-Identifier: GPL-3.0-only
-- The content-packed preset retains the QML bar's fractional-pixel geometry.
local function instance(iid,widget,host,region,start,settings)
    return {iid=iid,widget=widget,host=host,slot={region=region,start=start,span=1},settings=settings or {}}
end
return function(width)
    local tiles=math.max(6,math.floor((width or 1920)/32+.5))
    local pill=instance("status-pill","pill","top_bar","right",0)
    pill.children={
        instance("updates","updates","status-pill","left",0),
        instance("sound","sound","status-pill","centre",0),
        instance("wifi","wifi","status-pill","centre",1),
        instance("bluetooth","bluetooth","status-pill","centre",2),
        instance("battery","battery","status-pill","centre",3),
        instance("notifications","notifications","status-pill","centre",4),
        instance("clock","clock","status-pill","right",0),
        instance("power","power","status-pill","right",1),
    }
    return {version=1,preset="Nacre default",hosts={top_bar={tiles=tiles,mode="content",regions={left={start=0,span=tiles},centre={start=0,span=tiles},right={start=0,span=tiles}},instances={
        instance("launcher","launcher","top_bar","left",0,{icon="nacre",custom_path="",action="launcher",route=""}),
        instance("workspaces","workspaces","top_bar","left",1),
        instance("title","title","top_bar","centre",0),pill,
    }}}}
end
