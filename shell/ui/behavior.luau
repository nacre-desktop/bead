-- SPDX-License-Identifier: GPL-3.0-only
return {
  stores = {
    ["panel.open"] = true, ["launcher.apps"] = {},
    ["launcher.mode"]="apps", ["launcher.error"]="", ["launcher.pending"]=false, ["launcher.result_count"]=0, ["launcher.total_count"]=0,
    ["launcher.rail.order"]={}, ["launcher.rail.hidden"]={},
    ["launcher.selection"] = "", ["launcher.route"] = "apps", ["launcher.engine_route"]="",
    ["launcher.search.text"] = "", ["launcher.scroll.y"] = 0,
    ["launcher.conversion.value"]="", ["launcher.conversion.description"]="",
    ["launcher.conversion.error"]="", ["launcher.message"]="",
  },
  actions = {
    select_app = function(s,id) s["launcher.selection"] = id end,
    toggle_favorite = function(s,id) s["launcher.selection"] = id end,
    navigate = function(s,id)
      s["launcher.engine_route"]=id=="home" and "home" or ""
      s["launcher.route"]=(id=="home" or id=="apps") and "apps" or id
      s["launcher.mode"]=(id=="apps" or id=="home") and "apps" or "search"
      s["launcher.search.text"]=id=="actions" and "> " or ""
      s["launcher.selection"]=""
      s["launcher.scroll.y"]=0
    end,
    toggle = function(s) s["panel.open"] = not s["panel.open"] end,
  },
  computed = {
    ["launcher.results"]=function(s) return not s["launcher.grid"] end,
    ["launcher.subtitle"]=function(s) return s["launcher.grid"] and tr("bead.launcher.apps") or tr("bead.launcher.results") end,
    ["launcher.grid"]=function(s) return s["launcher.mode"]=="apps" and s["launcher.search.text"]:match("^%s*$")~=nil end,
    ["launcher.has_detail"]=function(s) return s["launcher.conversion.value"]~="" or s["launcher.conversion.description"]~="" or s["launcher.conversion.error"]~="" or s["launcher.message"]~="" end,
    ["launcher.empty"]=function(s) return s["launcher.result_count"]==0 and s["launcher.error"]=="" and not s["launcher.pending"] and not s["launcher.has_detail"] end,
    ["launcher.failed"]=function(s) return s["launcher.error"]~="" end,
    ["launcher.route_title"] = function(s)
      return s["launcher.engine_route"]=="home" and tr("bead.launcher.favorites") or ({apps=tr("bead.launcher.apps"),search=tr("bead.launcher.search-everything"),actions=tr("bead.launcher.actions")})[s["launcher.route"]] or tr("bead.launcher.apps")
    end,
    ["launcher.navigation"] = function(s)
      local rows={}
      local active=s["launcher.route"]
      if s["launcher.engine_route"]=="home" then active="home" end
      -- launcher-layout: hidden ids drop out (Home never), a configured order ranks the rest
      -- as Nacre's rail does (Home, the order, then the defaults); unranked rows keep their place at the end.
      local hidden={}
      for _,id in ipairs(s["launcher.rail.hidden"]) do hidden[id]=true end
      hidden.home=nil
      local rank,count={},0
      local configured=s["launcher.rail.order"]
      if #configured>0 then
        for _,id in ipairs({"home",table.unpack(configured)}) do
          if not rank[id] then count+=1 rank[id]=count end
        end
        for _,id in ipairs({"apps","actions"}) do
          if not rank[id] then count+=1 rank[id]=count end
        end
      end
      local ranked={}
      for index,item in {{id="apps",name=tr("bead.launcher.apps"),icon="apps"},{id="home",name=tr("bead.launcher.favorites"),icon="favorite"},{id="search",name=tr("bead.launcher.search"),icon="search"},{id="actions",name=tr("bead.launcher.actions"),icon="terminal"}} do
        if not hidden[item.id] then
          table.insert(ranked,{item=item,rank=rank[item.id] or 99,at=index})
        end
      end
      if count>0 then
        table.sort(ranked,function(a,b) if a.rank~=b.rank then return a.rank<b.rank end return a.at<b.at end)
      end
      for _,entry in ipairs(ranked) do
        table.insert(rows,{id=entry.item.id,name=entry.item.name,icon=entry.item.icon,selected=active==entry.item.id})
      end
      return rows
    end,
  },
}
