-- SPDX-License-Identifier: GPL-3.0-only
-- Generic single-column viewport list. Auto-sized content plus bounded spacers
-- supports long lists without exceeding the SDK's 20,000px dimension limit.
local ui = require("bead.ui")
return function(p)
  local total = ui.model_count(p.model)
  local variable = type(p.row_height)=="function"
  local pitch = not variable and p.row_height+p.gap or 0
  local offsets = {0}
  if variable then
    local rows = ui.resolve_for_check(p.model)
    for i,item in rows do offsets[i+1]=offsets[i]+p.row_height(item)+p.gap end
  end
  local function offset(index) return variable and offsets[index+1] or index*pitch end
  local rows_extent = math.max(0,offset(total)-p.gap)
  local prefix = (p.before_height or 0)+(p.before_gap or 0)
  local extent = prefix+rows_extent
  local window_height=p.window_height or p.height
  local scroll = math.max(0,math.clamp(ui.resolve_for_check(p.scroll),0,math.max(0,extent-window_height))-prefix)
  local first = 0
  local count = total
  if variable then
    while first<total and offset(first+1)<scroll do first+=1 end
    first=math.max(0,first-1)
    local last=first
    while last<total and offset(last)<scroll+window_height do last+=1 end
    count=math.min(total,last+1)-first
  else
    first=math.max(0,math.floor(scroll/pitch)-1)
    count=math.min(total-first,math.ceil(window_height/pitch)+2)
  end
  local children = {}
  local function space(prefix, height)
    local index = 0
    while height > 0 do
      index=index+1
      local chunk=math.min(height,16000)
      children[#children+1]=ui.Box{id=prefix..index,width="fill",height=chunk,shrink=0}
      height=height-chunk
    end
  end
  if p.before then
    children[#children+1]=p.before
    space("prefix-gap",p.before_gap or 0)
  end
  space("before",offset(first))
  children[#children+1]=ui.Column{id="window",width="fill",gap=p.gap,shrink=0,
    ui.Repeat{id=p.repeat_id or "rows",model=p.model,key=p.key,first=first,count=count,render=p.render}}
  space("after",math.max(0,rows_extent-offset(first)-math.max(0,offset(first+count)-offset(first)-p.gap)))
  return ui.Scroll{id=p.id,scroll=p.scroll,width=p.width,height=p.height,grow=p.grow,
    position=p.position,left=p.left,top=p.top,
    ui.Column{id="content",width="fill",gap=0,shrink=0,table.unpack(children)}}
end
