-- SPDX-License-Identifier: GPL-3.0-only
return {stores={
    ["panel.open"]=true,["view.edge"]="top",["view.width"]=310,["view.height"]=332,["view.duration"]=0.23,
    ["session.enabled"]=false,["session.busy"]=false,["session.confirm"]="",["session.confirm_action"]="",
    ["session.message"]="",
},actions={
    close=function(s) s["panel.open"]=false end,
    -- The session-menu Runtime authorizes and handles these host actions.
    ["session.select"]=function(_) end,
    ["session.confirm"]=function(_) end,
    ["session.cancel"]=function(_) end,
},computed={}}
