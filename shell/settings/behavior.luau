-- SPDX-License-Identifier: GPL-3.0-only
-- Store defaults for the Settings window. The adapter projects every `settings.*` value;
-- navigation, search text and the edit field are plain view state.
return {stores={
    ["settings.page"]="",["settings.search"]="",["settings.edit.text"]="",
    ["settings.scroll"]=0,["settings.nav.scroll"]=0,
    ["settings.nav"]={},["settings.items"]={},
    ["settings.title"]="",["settings.description"]="",["settings.message"]="",
    ["settings.writable"]=false,["settings.readonly"]=false,["settings.searching"]=false,["settings.empty"]=false,
},actions={
    -- The Settings adapter authorizes and handles this host action (page, edit, set, apply).
    ["settings.do"]=function(_) end,
},computed={}}
