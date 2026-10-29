-- SPDX-License-Identifier: GPL-3.0-only
return {
  stores = {
    ["bead.scroll.wheel_step"] = 100,
    ["view.detached"]=false,["panel.open"] = true, ["launcher.apps"] = {},
    ["launcher.mode"]="apps", ["launcher.error"]="", ["launcher.pending"]=false, ["launcher.icon_placeholder"]=false, ["launcher.result_count"]=0, ["launcher.total_count"]=0,
    ["launcher.rail.order"]={}, ["launcher.rail.hidden"]={}, ["launcher.groups"]={},
    ["launcher.selection"] = "", ["launcher.route"] = "apps", ["launcher.engine_route"]="",
    ["launcher.search.text"] = "", ["launcher.result_query"] = "", ["launcher.scroll.y"] = 0,
    ["launcher.conversion.value"]="", ["launcher.conversion.description"]="",
    ["launcher.conversion.error"]="", ["launcher.message"]="",
    ["launcher.quick"]={}, ["launcher.quick_error"]="", ["launcher.updates.count"]=-1,
    ["launcher.plugin.active"]=false, ["launcher.plugin.source"]="", ["launcher.plugin.rows"]={},
    ["launcher.plugin.notice"]="", ["plugins.message"]="",
    -- The inline form (B0199): `launcher_form.rs` opens it (kind, prompt, starting values) and runs its backend.
    ["launcher.form.kind"]="", ["launcher.form.message"]="", ["launcher.form.error"]="", ["launcher.form.busy"]=false,
    ["launcher.form.v1"]="", ["launcher.form.v2"]="", ["launcher.form.v3"]="", ["launcher.form.v4"]="",
  },
  actions = {
    select_app = function(s,id) s["launcher.selection"] = id end,
    toggle_favorite = function(s,id) s["launcher.selection"] = id end,
    ["launcher.quick"] = function(_) end,
    -- B0208: `>plugin` mode; the host installs, enables and updates (spec 0203 plugin state).
    ["plugins.do"] = function(_) end,
    -- Submit and cancel are the host's (`launcher.form`); a choice is only a store write ("<slot>:<value>").
    ["launcher.form"] = function(_) end,
    ["launcher.form.choose"] = function(s,arg)
      local slot,value=tostring(arg):match("^([1-4]):(.+)$")
      if slot then s["launcher.form.v"..slot]=value end
    end,
    navigate = function(s,id)
      s["launcher.engine_route"]=(id=="apps" or id=="actions") and "" or id
      s["launcher.route"]=(id=="actions") and id or "apps"
      s["launcher.mode"]=(id=="actions") and "search" or "apps"
      s["launcher.search.text"]=id=="actions" and "> " or ""
      s["launcher.selection"]=""
      s["launcher.scroll.y"]=0
    end,
    toggle = function(s) s["panel.open"] = not s["panel.open"] end,
  },
  computed = {
    ["launcher.form.has_error"]=function(s) return s["launcher.form.error"]~="" end,
    -- Keep the last accepted model's geometry while its replacement is in flight.
    ["launcher.display_query"]=function(s) return s["launcher.pending"] and s["launcher.result_query"] or s["launcher.search.text"] end,
    ["launcher.quick_allowed"]=function(s) return not s["launcher.display_query"]:match("^%s*=") end,
    ["launcher.results"]=function(s) return not s["launcher.grid"] end,
    ["launcher.home"]=function(s) return (s["launcher.engine_route"]:match("^[^.]+"))=="home" and s["launcher.display_query"]:match("^%s*$")~=nil end,
    ["launcher.hints"]=function(s) return s["launcher.display_query"]=="" end,
    ["launcher.empty_text"]=function(s) return (s["launcher.engine_route"]:match("^[^.]+"))=="home" and s["launcher.display_query"]:match("^%s*$")~=nil and tr("bead.launcher.home-empty") or tr("bead.launcher.no-results") end,
    ["launcher.subtitle"]=function(s) if s["launcher.home"] then return tr("bead.launcher.favorites") end return s["launcher.grid"] and tr("bead.launcher.apps") or tr("bead.launcher.results") end,
    ["launcher.grid"]=function(s) return s["launcher.mode"]=="apps" and s["launcher.display_query"]:match("^%s*$")~=nil end,
    ["launcher.has_detail"]=function(s) return s["launcher.conversion.value"]~="" or s["launcher.conversion.description"]~="" or s["launcher.conversion.error"]~="" or s["launcher.message"]~="" end,
    ["launcher.empty"]=function(s) return s["launcher.result_count"]==0 and s["launcher.error"]=="" and not s["launcher.pending"] and not s["launcher.has_detail"] end,
    ["launcher.failed"]=function(s) return s["launcher.error"]~="" end,
    ["launcher.route_title"] = function(s)
      local group=s["launcher.engine_route"]:match("^[^.]+")
      if group then
        for _,entry in ipairs(s["launcher.groups"]) do
          if entry.id==group then return entry.name end
        end
        if group=="home" then return tr("bead.launcher.home") end
      end
      return ({apps=tr("bead.launcher.apps"),search=tr("bead.launcher.search-everything"),actions=tr("bead.launcher.actions")})[s["launcher.route"]] or tr("bead.launcher.apps")
    end,
    ["launcher.navigation"] = function(s)
      local rows={}
      local route=s["launcher.engine_route"]
      if route=="" then route=s["launcher.route"] end
      local active=route:match("^[^.]+") or route
      -- The engine's `menu.groups` rail (Nacre's default order); before it arrives, or from an older daemon,
      -- the groups every engine has.
      local groups=s["launcher.groups"]
      if #groups==0 then
        groups={{id="home",name=tr("bead.launcher.favorites"),icon="home"},{id="apps",name=tr("bead.launcher.apps"),icon="apps"},{id="actions",name=tr("bead.launcher.actions"),icon="bolt"}}
      end
      -- launcher-layout as Nacre's rail(): Home, the configured order, then the defaults; hidden ids drop out
      -- (Home never); unranked rows go last in their own order.
      local hidden={}
      for _,entry in ipairs(s["launcher.rail.hidden"]) do hidden[entry.id]=true end
      hidden.home=nil
      local rank,count={},0
      local function place(id) if not rank[id] then count+=1 rank[id]=count end end
      place("home")
      for _,entry in ipairs(s["launcher.rail.order"]) do place(entry.id) end
      for _,id in ipairs({"home","apps","actions","style","setup","software","health","learn","power"}) do place(id) end
      local ranked={}
      for index,item in ipairs(groups) do
        if not hidden[item.id] then
          table.insert(ranked,{item=item,rank=rank[item.id] or 99,at=index})
        end
      end
      table.sort(ranked,function(a,b) if a.rank~=b.rank then return a.rank<b.rank end return a.at<b.at end)
      for _,entry in ipairs(ranked) do
        table.insert(rows,{id=entry.item.id,name=entry.item.name,icon=entry.item.icon,selected=active==entry.item.id})
      end
      return rows
    end,
  },
}
