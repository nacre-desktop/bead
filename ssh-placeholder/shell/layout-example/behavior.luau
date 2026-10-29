-- SPDX-License-Identifier: GPL-3.0-only
export type App = { id: string, name: string, selected: boolean }
return {
    stores = {
        ["launcher.query"] = "", ["launcher.scroll"] = 0,
        ["launcher.selection"] = "", ["launcher.route"] = "home",
        ["launcher.catalog"] = {
            {id="terminal",name="Terminal"}, {id="files",name="Files"},
            {id="browser",name="Browser"}, {id="settings",name="Settings"},
        },
    },
    actions = {
        select=function(s,id) s["launcher.selection"]=id end,
        navigate=function(s,id) s["launcher.route"]=id end,
    },
    computed = {
        ["launcher.apps"]=function(s)
            local rows={};local query=string.lower(s["launcher.query"])
            for _,item in s["launcher.catalog"] do
                if string.find(string.lower(item.name),query,1,true) then
                    table.insert(rows,{id=item.id,name=item.name,selected=s["launcher.selection"]==item.id})
                end
            end
            return rows
        end,
        ["launcher.navigation"]=function(s)
            local rows={}
            for _,item in {{id="home",name="Home"},{id="applications",name="Applications"},{id="system",name="System"}} do
                table.insert(rows,{id=item.id,name=item.name,selected=s["launcher.route"]==item.id})
            end
            return rows
        end,
    },
}
