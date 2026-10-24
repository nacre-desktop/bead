-- SPDX-License-Identifier: GPL-3.0-only
local function body_height(s)
  return s["extras.body_height"] or 424
end

local function wallpaper_item_matches(item,s)
  local animated=item.animated==true
  local dynamic=(s["extras.wallpaper.kind"] or "static") == "dynamic"
  if dynamic then
    if item.kind~="video" and not animated then return false end
  elseif item.kind~="image" or animated then return false end
  local text=(tostring(item.title or "").." "..tostring(item.id or "")):lower()
  for word in tostring(s["extras.query"] or ""):lower():gmatch("%S+") do if not text:find(word,1,true) then return false end end
  return true
end

local function normalized_selection(s)
  if s["extras.kind"] == "wallpaper" then
    local selected_id=s["extras.wallpaper.selection_id"] or ""
    local visible=0
    for _,item in ipairs(s["extras.rows"] or {}) do
      if wallpaper_item_matches(item,s) then
        visible=visible+1
        if selected_id~="" and item.id==selected_id then return visible end
      end
    end
    visible=0
    for _,item in ipairs(s["extras.rows"] or {}) do
      if wallpaper_item_matches(item,s) then
        visible=visible+1
        if item.active==true then return visible end
      end
    end
    return math.max(1,math.min(s["extras.selection"] or 1,visible))
  end
  if s["extras.selection_query"] ~= (s["extras.query"] or "") then
    return 1
  end
  return math.max(1,s["extras.selection"] or 1)
end

local function filtered(s)
  local kind, query = s["extras.kind"], tostring(s["extras.query"] or ""):lower()
  local source = kind == "overview" and (s["extras.windows"] or {}) or (s["extras.rows"] or {})
  local out, selection = {}, normalized_selection(s)
  for _, source_item in ipairs(source) do
    local include = true
    if kind == "overview" then
      include = ((tostring(source_item.title or "").." "..tostring(source_item.app_id or "")):lower()):find(query,1,true) ~= nil
    elseif kind == "keybindings" then
      include = ((tostring(source_item.key or "").." "..tostring(source_item.description or "")):lower()):find(query,1,true) ~= nil
    elseif kind == "wallpaper" then
      include = wallpaper_item_matches(source_item,s)
    end
    -- Palette rows arrive already filtered and ordered by menu.query. Luau must not
    -- change the engine's query semantics, ranking, or result order.
    if include then
      local item = {}; for k, v in pairs(source_item) do item[k] = v end
      item.selection_index = #out+1
      item.selected = kind ~= "keybindings" and item.selection_index == selection
      out[#out+1] = item
    end
  end
  return out
end

local function groups(s)
  if s["extras.available"] == false then return {} end
  local visible, by_id, result, workspaces = filtered(s), {}, {}, {}
  for _, ws in ipairs(s["extras.workspaces"] or {}) do
    local entry = { id = ws.id, index = ws.index, name = ws.name or "", output = ws.output,
      workspace = ws, windows = {} }
    by_id[ws.id] = entry
    workspaces[#workspaces+1] = entry
  end
  if s["extras.per_output"] ~= true then
    local present = {}
    for _, ws in ipairs(workspaces) do present[ws.index] = true end
    local limit = math.max(0,math.floor(s["extras.workspace_count"] or 7))
    for index=1,limit do
      if not present[index] then
        local id = "__default:"..tostring(index)
        local entry = { id=id, index=index, name="", workspace={id=id,index=index,name=""}, windows={} }
        by_id[id] = entry
        workspaces[#workspaces+1] = entry
      end
    end
  end
  for _, window in ipairs(visible) do
    local group = by_id[window.workspace]
    if group then group.windows[#group.windows+1] = window end
  end
  table.sort(workspaces,function(a,b)
    if a.index ~= b.index then return (a.index or 0)<(b.index or 0) end
    return tostring(a.id)<tostring(b.id)
  end)
  for _, ws in ipairs(workspaces) do
    local group = by_id[ws.id]
    if group and (s["extras.per_output"] == true or (ws.index > 0 and
      (ws.index <= (s["extras.workspace_count"] or 7) or #group.windows > 0))) then result[#result+1] = group end
  end
  return result
end

local function step(s, delta)
  local kind = s["extras.kind"]
  local query = s["extras.query"] or ""
  if s["extras.selection_query"] ~= query then
    s["extras.selection"] = kind == "wallpaper" and normalized_selection(s) or 1
    s["extras.selection_query"] = query
    s["extras.scroll"] = 0
  end
  if kind == "keybindings" then
    local rows = filtered(s)
    local step_size = s["extras.scroll_step"] or 47
    local extent = math.max(0,#rows*step_size-(s["extras.shortcuts_row_gap"] or 5))
    local maximum = math.max(0,extent-body_height(s))
    s["extras.scroll"] = math.clamp((s["extras.scroll"] or 0)+delta*step_size,0,maximum)
    return
  end
  local count = #filtered(s)
  local index = count == 0 and 1 or (kind == "wallpaper" and ((normalized_selection(s)-1+delta)%count)+1 or math.clamp(normalized_selection(s)+delta,1,count))
  s["extras.selection"] = index
  if kind == "wallpaper" then
    local item=filtered(s)[index]
    s["extras.wallpaper.selection_id"]=item and item.id or ""
    if item then s["extras.request"]="wallpaper:activate:"..tostring(item.id) end
  end
  if kind == "palette" and count > 0 then
    local pitch = s["extras.palette_row_pitch"] or 64
    local top = (index-1)*pitch
    local bottom = top+(s["extras.palette_row_height"] or 58)
    local scroll = s["extras.scroll"] or 0
    if top < scroll then scroll = top elseif bottom > scroll+body_height(s) then scroll = bottom-body_height(s) end
    s["extras.scroll"] = math.max(0,scroll)
  end
end

local function choose(s)
  local kind = s["extras.kind"]
  if kind == "keybindings" then return end
  local query = s["extras.query"] or ""
  if s["extras.selection_query"] ~= query then
    s["extras.selection"] = kind == "wallpaper" and normalized_selection(s) or 1
    s["extras.selection_query"] = query
  end
  local item = filtered(s)[normalized_selection(s)]
  if not item or item.disabled == true then return end
  if kind == "wallpaper" then s["extras.wallpaper.selection_id"]=item.id end
  if kind == "overview" then s["extras.request"] = "focus:"..tostring(item.id)
  elseif kind == "wallpaper" then s["extras.request"] = "wallpaper:activate:"..tostring(item.id)
  elseif kind == "palette" then s["extras.request"] = "activate:"..tostring(item.id) end
end

local exports = {
  stores = {
    ["panel.open"] = false, ["extras.kind"] = "overview", ["extras.windows"] = {},
    ["extras.workspaces"] = {}, ["extras.rows"] = {}, ["extras.available"] = true,
    ["extras.per_output"] = false, ["extras.workspace_count"] = 7,
    ["extras.message"] = "", ["extras.query"] = "", ["extras.scroll"] = 0,
    ["extras.wallpaper.kind"] = "static", ["extras.wallpaper.layout"] = "carousel",
    ["extras.wallpaper.revision"] = 0,
    ["extras.wallpaper.resource_revision"] = 0,
    ["extras.wallpaper.loading"] = false,
    ["extras.wallpaper.selection_id"] = "",
    ["extras.wallpaper.carousel_scroll"] = 0,
    ["extras.viewport.width"] = 1440, ["extras.viewport.height"] = 900,
    ["extras.body_height"] = 424, ["extras.palette_row_pitch"] = 64,
    ["extras.palette_row_height"] = 58, ["extras.scroll_step"] = 47,
    ["extras.shortcuts_row_gap"] = 5,
    ["extras.selection"] = 1, ["extras.selection_query"] = "", ["extras.request"] = "",
    ["extras.do"] = "",
  },
  computed = {
    ["extras.filtered"] = filtered,
    ["extras.wallpaper.backdrop"] = function(s)
      for _, item in ipairs(filtered(s)) do
        if item.selected then
          local path=item.preview and item.preview~="" and item.preview or item.thumbnail or ""
          return path or ""
        end
      end
      return ""
    end,
    ["extras.wallpaper.backdrop_visible"] = function(s)
      return (s["extras.wallpaper.backdrop"] or "") ~= ""
    end,
    ["extras.groups"] = groups,
    ["extras.empty"] = function(s)
      return s["extras.kind"] == "keybindings" and s["extras.available"] ~= false and #filtered(s) == 0
    end,
    ["extras.empty_message"] = function(s)
      if s["extras.available"] == false then
        if s["extras.message"] ~= "" then return s["extras.message"] end
        return "Unavailable"
      end
      if s["extras.kind"] == "wallpaper" then
        if s["extras.wallpaper.loading"] == true then return "Loading wallpapers…" end
        if #filtered(s) == 0 then return #(s["extras.rows"] or {}) == 0 and "No wallpapers available" or "No matching wallpapers" end
      end
      if s["extras.kind"] == "keybindings" then return "No matching shortcuts" end
      return "No matching items"
    end,
    ["extras.status_message"] = function(s)
      if s["extras.message"] ~= "" then return s["extras.message"] end
      if s["extras.available"] == false then return "Unavailable" end
      return ""
    end,
    ["extras.status_visible"] = function(s)
      return s["extras.message"] ~= "" or s["extras.available"] == false
    end,
    ["extras.title"] = function(s)
      return ({overview="Windows", palette="Commands", keybindings="Keyboard shortcuts", wallpaper="Wallpapers"})[s["extras.kind"]] or "Extras"
    end,
    ["extras.placeholder"] = function(s)
      return ({overview="Search windows or applications", palette="Actions, apps, windows, or brain: something", keybindings="Search a shortcut or action", wallpaper="Search wallpapers"})[s["extras.kind"]] or "Search"
    end,
  },
  actions = {
    ["extras.next"] = function(s) step(s,1) end,
    ["extras.prev"] = function(s) step(s,-1) end,
    ["extras.choose"] = choose,
    ["extras.close"] = function(s) s["panel.open"] = false end,
    ["close"] = function(s) s["panel.open"] = false end,
    ["extras.select"] = function(s, id)
      local query = s["extras.query"] or ""
      s["extras.selection_query"] = query
      if s["extras.kind"] == "wallpaper" then s["extras.wallpaper.selection_id"] = id end
      for index, item in ipairs(filtered(s)) do
        if item.id == id then s["extras.selection"] = index; return end
      end
    end,
    -- Runtime intercepts this registered daemon action argument and calls Extras::act.
    ["extras.do"] = function(_) end,
  },
}

-- Workspace-local scroll offsets are bounded and die with the open panel.
for index=1,64 do exports.stores["extras.window_scroll."..index]=0 end
return exports
