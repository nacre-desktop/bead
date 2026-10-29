-- SPDX-License-Identifier: GPL-3.0-only
-- Selector prompt (B0093). Filtering, ranking and keyboard selection are view state; `prompt.rs` projects the
-- request (kind, title, rows) and carries out the answer. A row id is "c<index>": the answer is looked up on
-- the host side, so the view never names a value.
local function norm(v)
  local t = tostring(v or ""):lower():gsub("[-_:]", " "):gsub("%s+", " ")
  return (t:match("^%s*(.-)%s*$"))
end
local function has_all(words, text)
  for _, word in ipairs(words) do
    if not text:find(word, 1, true) then return false end
  end
  return true
end
-- The launcher's ranking (menu-model.js matchScore) without its typo tolerance.
local function score(row, term, words)
  local name = norm(row.name)
  if name == term then return 120 end
  if name:sub(1, #term) == term then return 100 end
  if has_all(words, name) then return 85 end
  if #term >= 2 then
    local initials = ""
    for word in name:gmatch("[a-z0-9]+") do initials = initials .. word:sub(1, 1) end
    if initials:sub(1, #term) == term then return 80 end
  end
  if has_all(words, norm(tostring(row.name) .. " " .. tostring(row.description))) then return 55 end
  return -1
end

local function matches(s)
  local rows = s["prompt.rows"] or {}
  local term = norm(s["prompt.query"])
  if term == "" then return rows end
  local words = {}
  for word in term:gmatch("%S+") do words[#words + 1] = word end
  local hits = {}
  for index, row in ipairs(rows) do
    local value = score(row, term, words)
    if value >= 0 then hits[#hits + 1] = { row = row, index = index, score = value } end
  end
  table.sort(hits, function(a, b)
    if a.score ~= b.score then return a.score > b.score end
    return a.index < b.index
  end)
  local out = {}
  for i = 1, math.min(#hits, 100) do out[i] = hits[i].row end
  return out
end

-- The chosen row, or the first enabled one (Menu.firstEnabled) while nothing was chosen.
local function current(s)
  local list, id = matches(s), s["prompt.selected"] or ""
  if id ~= "" then
    for index, row in ipairs(list) do
      if row.id == id then return id, index, list end
    end
  end
  for index, row in ipairs(list) do
    if not row.disabled then return row.id, index, list end
  end
  return "", 0, list
end

-- Menu.step: move over disabled rows, stay put at the ends.
local function step(s, delta)
  local _, index, list = current(s)
  if #list == 0 then return end
  local next = index == 0 and (delta < 0 and #list or 1) or index + delta
  while next >= 1 and next <= #list do
    if not list[next].disabled then
      s["prompt.selected"] = list[next].id
      local pitch, height, row = s["prompt.row_pitch"], s["prompt.view_height"], s["prompt.row_height"]
      local top = (next - 1) * pitch
      local scroll = s["prompt.scroll"] or 0
      if top < scroll then scroll = top elseif top + row > scroll + height then scroll = top + row - height end
      s["prompt.scroll"] = math.max(0, scroll)
      return
    end
    next = next + (delta < 0 and -1 or 1)
  end
end

local function choose(s)
  if s["prompt.kind"] == "input" then
    s["prompt.request"] = "submit"
    return
  end
  local id, index, list = current(s)
  if index > 0 and not list[index].disabled then s["prompt.request"] = "choose:" .. id end
end

return {
  stores = {
    ["panel.open"] = true, ["prompt.kind"] = "pick", ["prompt.title"] = "", ["prompt.rows"] = {},
    ["prompt.query"] = "", ["prompt.input"] = "", ["prompt.selected"] = "", ["prompt.scroll"] = 0,
    ["prompt.error"] = "", ["prompt.request"] = "",
    ["prompt.row_pitch"] = 58, ["prompt.row_height"] = 55, ["prompt.view_height"] = 436,
    ["prompt.do"] = "",
  },
  computed = {
    ["prompt.matches"] = matches,
    ["prompt.current"] = function(s) return (current(s)) end,
    ["prompt.empty"] = function(s) return #matches(s) == 0 end,
  },
  actions = {
    ["prompt.next"] = function(s) step(s, 1) end,
    ["prompt.prev"] = function(s) step(s, -1) end,
    ["prompt.choose"] = choose,
    -- The prompt host authorizes and answers this registered action ("choose:ID", "submit", "cancel").
    ["prompt.do"] = function(_) end,
  },
}
