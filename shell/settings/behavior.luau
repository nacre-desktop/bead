-- SPDX-License-Identifier: GPL-3.0-only
-- Store defaults for the Settings window. The adapter projects every `settings.*` value;
-- navigation, search text and the edit field are plain view state.
return {stores={
    ["settings.page"]="",["settings.search"]="",["settings.edit.text"]="",
    ["settings.scroll"]=0,["settings.nav.scroll"]=0,["settings.select"]="",["settings.select.scroll"]=0,
    ["settings.nav"]={},["settings.items"]={},
    ["settings.title"]="",["settings.description"]="",["settings.message"]="",["settings.notice"]="",["settings.noticed"]=false,
    ["settings.pick.text"]="",["settings.secret"]="",["settings.reveal"]=false,
    ["settings.writable"]=false,["settings.readonly"]=false,["settings.searching"]=false,["settings.empty"]=false,
},actions={
    -- The Settings adapter authorizes and handles this host action (page, edit, set, apply).
    ["settings.do"]=function(_) end,
    -- A drop-down's field: opens its list (one at a time) or closes it again.
    ["settings.dropdown"]=function(s,id)
        s["settings.select"]=s["settings.select"]==id and "" or id
        s["settings.select.scroll"]=0
    end,
},computed={}}
