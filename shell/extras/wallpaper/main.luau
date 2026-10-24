-- SPDX-License-Identifier: GPL-3.0-only
-- Wallpaper picker presentation. Nacre routes and settings writes stay in extras.rs.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=require("../tokens")
local viewport_w=ui.resolve_for_check(ui.bind("extras.viewport.width")) or T.panel.default_screen_height
local viewport_h=ui.resolve_for_check(ui.bind("extras.viewport.height")) or T.panel.default_screen_height
local layout=ui.resolve_for_check(ui.bind("extras.wallpaper.layout")) or "carousel"
local kind=ui.resolve_for_check(ui.bind("extras.wallpaper.kind")) or "static"
local carousel=layout=="carousel"
local width=carousel and math.max(320,math.min(T.wallpaper.carousel_width,viewport_w-96)) or viewport_w
local height=carousel and T.wallpaper.carousel_height or viewport_h
local visible=ui.resolve_for_check(ui.bind("extras.wallpaper.visible")) or {}
local rows=ui.resolve_for_check(ui.bind("extras.filtered")) or {}
local count=#rows
local selected
for _,item in ipairs(rows) do if item.selected then selected=item; break end end
local toolbar_y=carousel and T.wallpaper.carousel_top or T.wallpaper.fullscreen_top
local toolbar_h=T.wallpaper.toolbar_height
local query_y=toolbar_y+toolbar_h+T.wallpaper.toolbar_gap
local search_h=T.wallpaper.search_height
local body_y=layout=="spotlight" and query_y+search_h+T.wallpaper.spotlight_top_gap
  or layout=="hexagons" and query_y+search_h+T.wallpaper.hex_top_gap
  or query_y+search_h+10
local body_h=math.max(0,height-body_y-(layout=="spotlight" and T.wallpaper.spotlight_body_clearance or T.wallpaper.fullscreen_nav_clearance))
local body_w=width-48
-- A normalized polygon approximates the QML rounded image clip within a pixel.
local function rounded_clip(w,h)
  local r=math.min(T.wallpaper.image_radius,w/2,h/2)
  local points={}
  for corner,center in ipairs{{w-r,r},{w-r,h-r},{r,h-r},{r,r}} do
    for step=0,7 do
      local angle=(corner-2)*math.pi/2+step*math.pi/14
      points[#points+1]={math.clamp((center[1]+r*math.cos(angle))/w,0,1),math.clamp((center[2]+r*math.sin(angle))/h,0,1)}
    end
  end
  return points
end
local hex_clip={{.25,0},{.75,0},{1,.5},{.75,1},{.25,1},{0,.5}}

local function button(id,label,active,request)
  local label_w=(utf8.len(label) or #label)*T.wallpaper.choice_advance
  return kit.Button{id=id,aria_label=label,width=label_w+2*T.wallpaper.choice_padding,shrink=0,height=toolbar_h,padding=0,
    padding_left=T.wallpaper.choice_padding,padding_right=T.wallpaper.choice_padding,
    radius=T.panel.close_radius,fill=active and "primary" or "surfaceContainerHigh",
    hover_fill=active and "primary" or "surfaceContainerHigh",activate_on_release=true,on_activate=ui.action("extras.do",request),
    ui.Row{width=label_w,height=toolbar_h,align="center",justify="center",
      ui.Text{text=label,width=label_w,height=math.ceil(T.palette.description_font*1.4),font_size=T.palette.description_font,
        text_align="center",color=active and "onPrimary" or "onSurface"}}}
end
local function nav_button(id,label,glyph,action)
  return kit.Button{id=id,aria_label=label,width=T.wallpaper.nav_width,height=T.wallpaper.nav_height,padding=0,
    radius=T.panel.close_radius,fill="surfaceContainerHigh",hover_fill="surfaceContainerHigh",
    activate_on_release=true,on_activate=count>0 and ui.action(action) or nil,
    ui.Row{width=T.wallpaper.nav_width,height=T.wallpaper.nav_height,align="center",justify="center",
      ui.Text{text=glyph,width=20,height=23,family="Material Symbols Rounded",font_size=16,color="onSurface"}}}
end

local function card(item, card_w, card_h, x, y, selected_card, clip)
  local hex=clip==hex_clip
  local image_inset=hex and 0 or T.wallpaper.image_inset
  local image_h=card_h-image_inset*2
  local kids={}
  local path=selected_card and not carousel and item.preview~="" and item.preview or item.thumbnail
  if path and path~="" then
    kids[#kids+1]=ui.Image{id="thumbnail",position="absolute",left=image_inset,top=image_inset,width=card_w-image_inset*2,height=image_h,
      icon="wallpaper:"..path,image_fit="cover",clip_polygon=hex and hex_clip or rounded_clip(card_w-image_inset*2,image_h),aria_label=item.title or "Wallpaper"}
  else
    kids[#kids+1]=ui.Text{id="missing-thumbnail",position="absolute",left=0,top=0,width=card_w,height=image_h,
      text=item.kind=="video" and "movie" or "image",family="Material Symbols Rounded",font_size=36,
      color="onSurfaceVariant",text_align="center"}
  end
  if clip==hex_clip then
    kids[#kids+1]=ui.Image{id="hex-outline",position="absolute",left=0,top=0,width=card_w,height=card_h,
      path=selected_card and "extras/wallpaper/assets/hex-selected.themed.svg" or "extras/wallpaper/assets/hex-idle.themed.svg"}
    kids[#kids+1]=ui.Text{id="hex-title",position="absolute",left=card_w*.25,top=card_h-30,
      width=card_w*.5,height=20,text=item.title or "",font_size=T.palette.description_font,
      text_align="center",color=T.wallpaper.photo_ink}
  end
  return ui.Button{id="wallpaper-"..tostring(item.selection_index),position="absolute",left=x,top=y,
    width=card_w,height=card_h,padding=0,radius=T.wallpaper.card_radius,
    fill=hex and "transparent" or "surfaceContainer",
    border_width=hex and 0 or selected_card and 2 or 0,border_color=selected_card and "primary" or "outlineVariant",
    hover_fill=hex and "transparent" or "surfaceContainer",clip_polygon=hex and hex_clip or nil,activate_on_release=true,
    focus_on_pointer=false,on_activate=ui.action("extras.do","wallpaper:select:"..tostring(item.id)),
    aria_label=(item.title or "Wallpaper")..(item.active and ", current" or ""),
    ui.Box{width="fill",height="fill",radius=T.wallpaper.card_radius,
      fill="transparent",table.unpack(kids)}}
end

local body
if layout=="carousel" then
  local available=math.max(0,width-48)
  local candidate=math.min(count,math.max(1,math.floor(((available/266-1)/2)+.5)*2+1))
  local slots=candidate>1 and candidate%2==0 and candidate-1 or candidate
  local card_w=math.min(280,(available-math.max(0,slots-1)*16)/math.max(1,slots))
  local card_h=math.max(0,math.min(height-body_y-T.wallpaper.carousel_body_clearance,card_w*.66))
  local strip_w=slots*card_w+math.max(0,slots-1)*T.wallpaper.card_gap
  local children={}
  for _,item in ipairs(visible) do
    local x=(strip_w-card_w)/2+(item.carousel_offset or 0)*(card_w+T.wallpaper.card_gap)
    children[#children+1]=card(item,card_w,card_h,x,0,item.selected,nil)
  end
  body=ui.Scroll{id="carousel-strip",position="absolute",left=(width-strip_w)/2,top=body_y,width=strip_w,height=card_h,
    scroll=ui.bind("extras.wallpaper.carousel_scroll"),ui.Box{width=strip_w,height=card_h,shrink=0,table.unpack(children)}}
elseif layout=="spotlight" then
  local hero_width=math.min(body_w*(count>1 and .5 or .8),1000,body_h*16/9)
  local side_width=hero_width*.62
  local area={}
  table.sort(visible,function(a,b) return math.abs(a.carousel_offset or 0)>math.abs(b.carousel_offset or 0) end)
  local side_slots=math.max(0,math.min(2,math.ceil((count-1)/2)))
  for _,item in ipairs(visible) do
    local offset=item.carousel_offset or 0
    local distance=math.abs(offset)
    local weight=math.max(0,1-distance)
    local card_w=side_width*math.max(.7,1-math.max(0,distance-1)*.15)+(hero_width-side_width)*weight
    local card_h=card_w*9/16
    local center_distance=hero_width*.54
    local x=24+body_w/2+math.sign(offset)*(math.min(1,distance)*center_distance+math.max(0,distance-1)*hero_width*.2)-card_w/2
    local y=body_y+(body_h-card_h)/2
    if distance<side_slots+1 then area[#area+1]=card(item,card_w,card_h,x,y,item.selected,nil) end
  end
  body=ui.Box{id="spotlight-strip",position="absolute",left=0,top=0,width=width,height=height,table.unpack(area)}
else
  local area_w=width-48
  local columns=math.max(3,math.min(7,math.floor(area_w/230)))
  local tile_w=math.min(340,(area_w-30)/(1+(columns-1)*.76))
  local tile_h=tile_w*.87
  local pitch=tile_h+12
  local extent=count>0 and math.ceil(count/columns)*pitch+tile_h*.5 or 0
  local tiles={}
  for _,item in ipairs(visible) do
    tiles[#tiles+1]=card(item,tile_w,tile_h,item.hex_x or 0,item.hex_y or 0,item.selected,hex_clip)
  end
  body=ui.Scroll{id="hexagon-scroll",position="absolute",left=24,top=body_y,width=area_w,height=body_h,
    scroll=ui.bind("extras.scroll"),ui.Box{id="hexagon-extent",width=area_w,height=extent,shrink=0,table.unpack(tiles)}}
end

local controls=ui.Row{id="wallpaper-controls",position="absolute",left=0,top=toolbar_y,width=width,height=toolbar_h,
  gap=T.wallpaper.group_gap,align="center",justify="center",
  ui.Row{id="media",height=toolbar_h,gap=T.wallpaper.toolbar_gap,shrink=0,
    button("static","Static",kind=="static","wallpaper:kind:static"),
    button("dynamic","Dynamic",kind=="dynamic","wallpaper:kind:dynamic")},
  ui.Box{id="media-divider",width=1,height=24,shrink=0,fill="outline"},
  ui.Row{id="layouts",height=toolbar_h,gap=T.wallpaper.toolbar_gap,shrink=0,
    button("carousel","Carousel",layout=="carousel","wallpaper:layout:carousel"),
    button("spotlight","Spotlight",layout=="spotlight","wallpaper:layout:spotlight"),
    button("hexagons","Hexagons",layout=="hexagons","wallpaper:layout:hexagons")},
  ui.Box{id="layout-divider",width=1,height=24,shrink=0,fill="outline"},
  kit.Button{id="add",aria_label="Add wallpapers unavailable",width=T.wallpaper.add_width,height=toolbar_h,padding=0,
    radius=T.panel.close_radius,fill=ui.alpha("surfaceContainerHigh",.4),
    ui.Row{width=T.wallpaper.add_width,height=toolbar_h,align="center",justify="center",gap=4,
      ui.Text{text="add",width=16,height=23,family="Material Symbols Rounded",font_size=16,color=ui.alpha("onSurface",.4)},
      ui.Text{text="Add",width=23,height=19,font_size=T.palette.description_font,color=ui.alpha("onSurface",.4)}}}}

local nav_title=selected and selected.title or ""
local nav=ui.Row{id="wallpaper-navigation",position="absolute",left=0,
  top=height-(layout=="spotlight" and T.wallpaper.spotlight_nav_y or T.wallpaper.nav_bottom+T.wallpaper.nav_height),
  width=width,height=T.wallpaper.nav_height,gap=14,align="center",justify="center",
  nav_button("previous","Previous wallpaper","chevron_left","extras.prev"),
  ui.Text{id="selected-title",width=math.min(420,width-300),height=21,text=nav_title,
    font_size=T.wallpaper.nav_font,text_align="center",color=carousel and "onSurface" or T.wallpaper.photo_ink},
  nav_button("next","Next wallpaper","chevron_right","extras.next")}

local backdrop=not carousel and ui.Image{id="gallery-backdrop",position="absolute",left=0,top=0,width=width,height=height,
  visible=ui.bind("extras.wallpaper.backdrop_visible"),icon=ui.bind("extras.wallpaper.backdrop"),image_fit="cover",aria_label="Selected wallpaper preview"} or nil
local scrim=not carousel and ui.Box{id="gallery-scrim",position="absolute",left=0,top=0,width=width,height=height,
  fill=ui.alpha("shadow",T.wallpaper.scrim_alpha)} or nil
local search_w=math.max(T.wallpaper.search_min_width,math.min(T.wallpaper.search_max_width,width-48))

local children={
  kit.TextInput{id="search",position="absolute",left=(width-search_w)/2,top=query_y,width=search_w,height=search_h,
    focus_ring=false,text=ui.bind("extras.query"),placeholder="Search wallpapers",padding_left=12,padding_right=12,
    padding_top=8,padding_bottom=8,radius=T.panel.search_radius,font_size=kit.tokens.text.body*4/3,
    fill="surfaceContainerHigh"},
  controls,body,nav,
  ui.Text{id="empty",position="absolute",left=24,top=height/2-12,width=width-48,height=24,
    text=ui.bind("extras.empty_message"),visible=#rows==0 and not ui.resolve_for_check(ui.bind("extras.status_visible")),font_size=T.panel.empty_font,
    text_align="center",color=carousel and "onSurface" or T.wallpaper.photo_ink},
  ui.Text{id="status",position="absolute",left=24,top=body_y+body_h/2+14,width=width-48,height=24,
    text=ui.bind("extras.status_message"),visible=ui.bind("extras.status_visible"),font_size=T.panel.empty_font,
    text_align="center",color="onSurfaceVariant"}}
if backdrop then table.insert(children,1,scrim);table.insert(children,1,backdrop) end
return kit.Panel{id="extras",edge="bottom",open=ui.bind("panel.open"),width=width,height=height,
  duration=T.panel.duration,transition=T.panel.transition,reveal=true,fade=false,radius=T.panel.radius,
  fill=carousel and "surface" or "transparent",border_width=carousel and T.panel.border or 0,
  border_color="outlineVariant",padding=0,gap=0,table.unpack(children)}
