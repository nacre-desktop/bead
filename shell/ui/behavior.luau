-- SPDX-License-Identifier: GPL-3.0-only
return {
  stores = {
    ["panel.open"] = true, ["launcher.apps"] = {},
    ["launcher.mode"]="apps", ["launcher.error"]="", ["launcher.result_count"]=0,
    ["launcher.selection"] = "", ["launcher.route"] = "apps", ["launcher.engine_route"]="",
    ["launcher.search.text"] = "", ["launcher.scroll.y"] = 0,
    ["launcher.conversion.value"]="", ["launcher.conversion.description"]="",
    ["launcher.conversion.error"]="", ["launcher.message"]="",
  },
  actions = {
    select_app = function(s,id) s["launcher.selection"] = id end,
    navigate = function(s,id)
      s["launcher.engine_route"]=""
      s["launcher.route"]=id
      s["launcher.mode"]=id=="apps" and "apps" or "search"
      s["launcher.search.text"]=id=="actions" and "> " or ""
      s["launcher.selection"]=""
      s["launcher.scroll.y"]=0
    end,
    toggle = function(s) s["panel.open"] = not s["panel.open"] end,
  },
  computed = {
    ["launcher.results"]=function(s) return not s["launcher.grid"] end,
    ["launcher.subtitle"]=function(s) return s["launcher.grid"] and "Applications" or "Results" end,
    ["launcher.grid"]=function(s) return s["launcher.mode"]=="apps" and s["launcher.search.text"]:match("^%s*$")~=nil end,
    ["launcher.has_detail"]=function(s) return s["launcher.conversion.value"]~="" or s["launcher.conversion.description"]~="" or s["launcher.conversion.error"]~="" or s["launcher.message"]~="" end,
    ["launcher.empty"]=function(s) return s["launcher.result_count"]==0 and s["launcher.error"]=="" and not s["launcher.has_detail"] end,
    ["launcher.failed"]=function(s) return s["launcher.error"]~="" end,
    ["launcher.route_title"] = function(s)
      return ({apps="Applications",search="Search everything",actions="Actions"})[s["launcher.route"]] or "Applications"
    end,
    ["launcher.navigation"] = function(s)
      local rows={}
      for _,item in {{id="apps",name="Applications",icon="apps"},{id="search",name="Search",icon="search"},{id="actions",name="Actions",icon="terminal"}} do
        table.insert(rows,{id=item.id,name=item.name,icon=item.icon,selected=s["launcher.route"]==item.id})
      end
      return rows
    end,
  },
}
