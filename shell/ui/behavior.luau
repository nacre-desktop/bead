-- SPDX-License-Identifier: GPL-3.0-only
return {
  stores = {
    ["launcher.open"] = true, ["launcher.catalog"] = {},
    ["launcher.selection"] = "", ["launcher.route"] = "home",
    ["launcher.search.text"] = "", ["launcher.scroll.y"] = 0,
  },
  actions = {
    select_app = function(s,id) s["launcher.selection"] = id end,
    navigate = function(s,id) s["launcher.route"] = id end,
    toggle = function(s) s["launcher.open"] = not s["launcher.open"] end,
  },
  computed = {
    ["launcher.route_title"] = function(s)
      return ({home="Home",applications="Applications",system="System",settings="Settings",files="Files",wallpaper="Wallpaper"})[s["launcher.route"]] or "Home"
    end,
    ["launcher.apps"] = function(s)
      local rows={}; local query=string.lower(s["launcher.search.text"])
      for _,app in s["launcher.catalog"] do
        if string.find(string.lower(app.name),query,1,true) then
          table.insert(rows,{id=app.id,name=app.name,icon=app.icon,selected=s["launcher.selection"]==app.id})
        end
      end
      return rows
    end,
    ["launcher.navigation"] = function(s)
      local rows={}
      for _,item in {{id="home",name="Home",icon="home"},{id="applications",name="Applications",icon="apps"},{id="system",name="System",icon="computer"},{id="settings",name="Settings",icon="settings"},{id="files",name="Files",icon="folder"},{id="wallpaper",name="Wallpaper",icon="wallpaper"}} do
        table.insert(rows,{id=item.id,name=item.name,icon=item.icon,selected=s["launcher.route"]==item.id})
      end
      return rows
    end,
  },
}
