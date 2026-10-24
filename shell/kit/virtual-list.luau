-- SPDX-License-Identifier: GPL-3.0-only
-- Generic single-column viewport list. Auto-sized content plus bounded spacers
-- supports long lists without exceeding the SDK's 20,000px dimension limit.
local ui = require("bead.ui")
return function(p)
  local total = ui.model_count(p.model)
  local pitch = p.row_height+p.gap
  local extent = math.max(0,total*pitch-p.gap)
  local scroll = math.clamp(ui.resolve_for_check(p.scroll),0,math.max(0,extent-p.height))
  local first = math.max(0,math.floor(scroll/pitch)-1)
  local count = math.min(total-first,math.ceil(p.height/pitch)+2)
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
  space("before",first*pitch)
  children[#children+1]=ui.Column{id="window",width="fill",gap=p.gap,shrink=0,
    ui.Repeat{id="rows",model=p.model,key=p.key,first=first,count=count,render=p.render}}
  space("after",math.max(0,extent-first*pitch-math.max(0,count*pitch-p.gap)))
  return ui.Scroll{id=p.id,scroll=p.scroll,width=p.width,height=p.height,
    ui.Column{id="content",width="fill",gap=0,shrink=0,table.unpack(children)}}
end
