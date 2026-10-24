-- SPDX-License-Identifier: GPL-3.0-only
-- Wallpaper picker presentation. Nacre routes and settings writes stay in extras.rs.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=require("../tokens")
local viewport_w=ui.resolve_for_check(ui.bind("extras.viewport.width")) or T.wallpaper.viewport_fallback_width
local viewport_h=ui.resolve_for_check(ui.bind("extras.viewport.height")) or T.wallpaper.viewport_fallback_height
local layout=ui.resolve_for_check(ui.bind("extras.wallpaper.layout")) or "carousel"
local kind=ui.resolve_for_check(ui.bind("extras.wallpaper.kind")) or "static"
local resource_revision=ui.resolve_for_check(ui.bind("extras.wallpaper.resource_revision")) or 0
local carousel=layout=="carousel"
local width=carousel and math.max(T.wallpaper.carousel_min_width,math.min(T.wallpaper.carousel_width,viewport_w-T.wallpaper.carousel_viewport_inset*2)) or viewport_w
local height=carousel and T.wallpaper.carousel_height or viewport_h
local rows=ui.resolve_for_check(ui.bind("extras.filtered")) or {}
local count=#rows
local selected
for _,item in ipairs(rows) do if item.selected then selected=item; break end end
local function circular_rows(limit)
  local visible={}
  local selection=selected and selected.selection_index or 1
  for index,item in ipairs(rows) do
    local offset=((index-selection+count/2)%count)-count/2
    if math.abs(offset)<=limit then
      item.carousel_offset=offset
      visible[#visible+1]=item
    end
  end
  return visible
end
local toolbar_y=carousel and T.wallpaper.carousel_top or T.wallpaper.fullscreen_top
local toolbar_h=T.wallpaper.toolbar_height
local query_y=toolbar_y+toolbar_h+T.wallpaper.toolbar_gap
local search_h=T.wallpaper.search_height
local body_y=layout=="spotlight" and query_y+search_h+T.wallpaper.spotlight_top_gap
  or layout=="hexagons" and query_y+search_h+T.wallpaper.hex_top_gap
  or query_y+search_h+T.wallpaper.carousel_content_gap
local body_h=math.max(0,height-body_y-(layout=="spotlight" and T.wallpaper.spotlight_body_clearance or T.wallpaper.fullscreen_nav_clearance))
local body_w=width-T.wallpaper.content_inset*2
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
  local line_h=math.ceil(T.palette.description_font*1.4)
  return kit.Button{id=id,aria_label=label,width=T.wallpaper.choice_widths[id],shrink=0,height=toolbar_h,padding=0,
    radius=T.panel.close_radius,fill=active and "primary" or "surfaceContainerHigh",
    hover_fill=active and "primary" or "surfaceContainerHigh",activate_on_release=true,on_activate=ui.action("extras.do",request),
    ui.Text{position="absolute",left=0,top=(toolbar_h-line_h)/2,width="fill",height=line_h,
      text=label,font_size=T.palette.description_font,text_align="center",color=active and "onPrimary" or "onSurface"}}
end
local function nav_button(id,label,glyph,action)
  return kit.Button{id=id,aria_label=label,width=T.wallpaper.nav_width,height=T.wallpaper.nav_height,padding=0,
    radius=T.panel.close_radius,fill="surfaceContainerHigh",hover_fill="surfaceContainerHigh",
    activate_on_release=true,on_activate=count>0 and ui.action(action) or nil,
    ui.Row{width=T.wallpaper.nav_width,height=T.wallpaper.nav_height,align="center",justify="center",
      ui.Text{text=glyph,width=T.wallpaper.nav_icon_width,height=T.wallpaper.nav_icon_height,family="Material Symbols Rounded",font_size=T.wallpaper.nav_icon_font,color="onSurface"}}}
end

local function card(item, card_w, card_h, x, y, selected_card, clip)
  local hex=clip==hex_clip
  local image_inset=hex and 0 or T.wallpaper.image_inset
  local image_h=card_h-image_inset*2
  local kids={}
  local path=selected_card and not carousel and item.preview~="" and item.preview or item.thumbnail
  if path and path~="" then
    kids[#kids+1]=ui.Image{id="thumbnail-"..tostring(resource_revision),position="absolute",left=image_inset,top=image_inset,width=card_w-image_inset*2,height=image_h,
      icon=path,image_fit="cover",clip_polygon=hex and hex_clip or rounded_clip(card_w-image_inset*2,image_h),aria_label=item.title or "Wallpaper"}
  else
    kids[#kids+1]=ui.Text{id="missing-thumbnail",position="absolute",left=0,top=0,width=card_w,height=image_h,
      text=item.kind=="video" and "movie" or "image",family="Material Symbols Rounded",font_size=T.wallpaper.thumbnail_fallback_icon,
      color="onSurfaceVariant",text_align="center"}
  end
  if clip==hex_clip then
    kids[#kids+1]=ui.Image{id="hex-outline",position="absolute",left=0,top=0,width=card_w,height=card_h,
      path=selected_card and "extras/wallpaper/assets/hex-selected.themed.svg" or "extras/wallpaper/assets/hex-idle.themed.svg"}
    kids[#kids+1]=ui.Text{id="hex-title",position="absolute",left=card_w*T.wallpaper.hex_caption_left_fraction,top=card_h-T.wallpaper.hex_caption_bottom,
      width=card_w*T.wallpaper.hex_caption_width_fraction,height=T.wallpaper.hex_caption_height,text=item.title or "",font_size=T.palette.description_font,
      text_align="center",color=T.wallpaper.photo_ink}
  end
  return ui.Button{id="wallpaper-"..tostring(item.selection_index),position="absolute",left=x,top=y,
    width=card_w,height=card_h,padding=0,radius=T.wallpaper.card_radius,
    fill=hex and "transparent" or "surfaceContainer",
    border_width=hex and 0 or selected_card and T.wallpaper.selected_border_width or 0,border_color=selected_card and "primary" or "outlineVariant",
    hover_fill=hex and "transparent" or "surfaceContainer",clip_polygon=hex and hex_clip or nil,activate_on_release=true,
    focus_on_pointer=false,on_activate=ui.action("extras.do","wallpaper:select:"..tostring(item.id)),
    aria_label=(item.title or "Wallpaper")..(item.active and ", current" or ""),
    ui.Box{width="fill",height="fill",radius=T.wallpaper.card_radius,
      fill="transparent",table.unpack(kids)}}
end

local body
if layout=="carousel" then
  local available=math.max(0,width-T.wallpaper.content_inset*2)
  local candidate=math.min(count,math.max(1,math.floor(((available/T.wallpaper.carousel_slot_estimate-1)/2)+.5)*2+1))
  local slots=candidate>1 and candidate%2==0 and candidate-1 or candidate
  local card_w=math.min(T.wallpaper.carousel_card_max_width,(available-math.max(0,slots-1)*T.wallpaper.card_gap)/math.max(1,slots))
  local card_h=math.max(0,math.min(height-body_y-T.wallpaper.carousel_body_clearance,card_w*T.wallpaper.carousel_card_height_ratio))
  local strip_w=slots*card_w+math.max(0,slots-1)*T.wallpaper.card_gap
  local children={}
  for _,item in ipairs(circular_rows(math.max(2,math.ceil(slots/2)+1))) do
    local x=(strip_w-card_w)/2+(item.carousel_offset or 0)*(card_w+T.wallpaper.card_gap)
    children[#children+1]=card(item,card_w,card_h,x,0,item.selected,nil)
  end
  body=ui.Scroll{id="carousel-strip",position="absolute",left=(width-strip_w)/2,top=body_y,width=strip_w,height=card_h,
    scroll=ui.bind("extras.wallpaper.carousel_scroll"),ui.Box{width=strip_w,height=card_h,shrink=0,table.unpack(children)}}
elseif layout=="spotlight" then
  local hero_width=math.min(body_w*(count>1 and T.wallpaper.spotlight_multiple_fraction or T.wallpaper.spotlight_single_fraction),T.wallpaper.spotlight_max,body_h*T.wallpaper.spotlight_aspect_ratio)
  local side_width=hero_width*T.wallpaper.spotlight_side_fraction
  local area={}
  local side_slots=math.max(0,math.min(T.wallpaper.spotlight_max_side_slots,math.ceil((count-1)/2)))
  local visible=circular_rows(side_slots+1)
  table.sort(visible,function(a,b) return math.abs(a.carousel_offset or 0)>math.abs(b.carousel_offset or 0) end)
  for _,item in ipairs(visible) do
    local offset=item.carousel_offset or 0
    local distance=math.abs(offset)
    local weight=math.max(0,1-distance)
    local card_w=side_width*math.max(T.wallpaper.spotlight_side_min_scale,1-math.max(0,distance-1)*T.wallpaper.spotlight_side_falloff)+(hero_width-side_width)*weight
    local card_h=card_w*T.wallpaper.spotlight_card_height_ratio
    local center_distance=hero_width*T.wallpaper.spotlight_center_offset
    local x=T.wallpaper.content_inset+body_w/2+math.sign(offset)*(math.min(1,distance)*center_distance+math.max(0,distance-1)*hero_width*T.wallpaper.spotlight_travel_offset)-card_w/2
    local y=body_y+(body_h-card_h)/2
    if distance<side_slots+1 then area[#area+1]=card(item,card_w,card_h,x,y,item.selected,nil) end
  end
  body=ui.Box{id="spotlight-strip",position="absolute",left=0,top=0,width=width,height=height,table.unpack(area)}
else
  local area_w=width-T.wallpaper.content_inset*2
  local columns=math.max(T.wallpaper.hex_min_columns,math.min(T.wallpaper.hex_max_columns,math.floor(area_w/T.wallpaper.hex_column_width)))
  local tile_w=math.min(T.wallpaper.hex_max_tile_width,(area_w-T.wallpaper.hex_side_padding)/(1+(columns-1)*T.wallpaper.hex_column_overlap))
  local tile_h=tile_w*T.wallpaper.hex_tile_height_ratio
  local pitch=tile_h+T.wallpaper.hex_gap
  local extent=count>0 and math.ceil(count/columns)*pitch+tile_h*T.wallpaper.hex_extent_tail or 0
  local tiles={}
  local scroll=ui.resolve_for_check(ui.bind("extras.scroll")) or 0
  local first=math.max(0,math.floor(scroll/pitch)-1)*columns+1
  local last=math.min(count,math.ceil((scroll+body_h)/pitch+1)*columns)
  for index=first,last do
    local item=rows[index]
    local x=(area_w-(tile_w+(columns-1)*tile_w*T.wallpaper.hex_column_overlap))/2+((index-1)%columns)*tile_w*T.wallpaper.hex_column_overlap
    local y=(index-1)//columns*pitch+((index-1)%columns%2)*tile_h*T.wallpaper.hex_extent_tail
    tiles[#tiles+1]=card(item,tile_w,tile_h,x,y,item.selected,hex_clip)
  end
  body=ui.Scroll{id="hexagon-scroll",position="absolute",left=T.wallpaper.content_inset,top=body_y,width=area_w,height=body_h,
    scroll=ui.bind("extras.scroll"),ui.Box{id="hexagon-extent",width=area_w,height=extent,shrink=0,table.unpack(tiles)}}
end

local controls=ui.Row{id="wallpaper-controls",position="absolute",left=0,top=toolbar_y,width=width,height=toolbar_h,
  gap=T.wallpaper.group_gap,align="center",justify="center",
  ui.Row{id="media",height=toolbar_h,gap=T.wallpaper.toolbar_gap,shrink=0,
    button("static","Static",kind=="static","wallpaper:kind:static"),
    button("dynamic","Dynamic",kind=="dynamic","wallpaper:kind:dynamic")},
  ui.Box{id="media-divider",width=T.wallpaper.divider_width,height=T.wallpaper.divider_height,shrink=0,fill="outline"},
  ui.Row{id="layouts",height=toolbar_h,gap=T.wallpaper.toolbar_gap,shrink=0,
    button("carousel","Carousel",layout=="carousel","wallpaper:layout:carousel"),
    button("spotlight","Spotlight",layout=="spotlight","wallpaper:layout:spotlight"),
    button("hexagons","Hexagons",layout=="hexagons","wallpaper:layout:hexagons")},
  ui.Box{id="layout-divider",width=T.wallpaper.divider_width,height=T.wallpaper.divider_height,shrink=0,fill="outline"},
  kit.Button{id="add",aria_label="Add wallpapers unavailable",width=T.wallpaper.add_width,height=toolbar_h,padding=0,
    radius=T.panel.close_radius,fill=ui.alpha("surfaceContainerHigh",.4),
    ui.Row{width=T.wallpaper.add_width,height=toolbar_h,align="center",justify="center",gap=4,
      ui.Text{text="add",width=T.wallpaper.add_icon_width,height=T.wallpaper.add_icon_height,family="Material Symbols Rounded",font_size=T.wallpaper.add_icon_font,color=ui.alpha("onSurface",.4)},
      ui.Text{text="Add",width=T.wallpaper.add_label_width,height=T.wallpaper.add_label_height,font_size=T.palette.description_font,color=ui.alpha("onSurface",.4)}}}}

local nav_title=selected and selected.title or ""
local nav=ui.Row{id="wallpaper-navigation",position="absolute",left=0,
  top=height-(layout=="spotlight" and T.wallpaper.spotlight_nav_y or T.wallpaper.nav_bottom+T.wallpaper.nav_height),
  width=width,height=T.wallpaper.nav_height,gap=T.wallpaper.nav_gap,align="center",justify="center",
  nav_button("previous","Previous wallpaper","chevron_left","extras.prev"),
  ui.Text{id="selected-title",width=math.min(T.wallpaper.nav_title_max_width,width-T.wallpaper.nav_title_reserved_width),height=T.wallpaper.nav_title_height,text=nav_title,
    font_size=T.wallpaper.nav_font,text_align="center",color=carousel and "onSurface" or T.wallpaper.photo_ink},
  nav_button("next","Next wallpaper","chevron_right","extras.next")}

local backdrop=not carousel and ui.Image{id="gallery-backdrop-"..tostring(resource_revision),position="absolute",left=0,top=0,width=width,height=height,
  visible=ui.bind("extras.wallpaper.backdrop_visible"),icon=ui.bind("extras.wallpaper.backdrop"),image_fit="cover",aria_label="Selected wallpaper preview"} or nil
local scrim=not carousel and ui.Box{id="gallery-scrim",position="absolute",left=0,top=0,width=width,height=height,
  fill=ui.alpha("shadow",T.wallpaper.scrim_alpha)} or nil
local search_w=math.max(T.wallpaper.search_min_width,math.min(T.wallpaper.search_max_width,width-T.wallpaper.content_inset*2))

local children={
  kit.TextInput{id="search",position="absolute",left=(width-search_w)/2,top=query_y,width=search_w,height=search_h,
    focus_ring=false,text=ui.bind("extras.query"),placeholder="Search wallpapers",padding_left=T.wallpaper.search_padding,padding_right=T.wallpaper.search_padding,
    padding_top=T.wallpaper.search_vertical,padding_bottom=T.wallpaper.search_vertical,radius=T.panel.search_radius,font_size=kit.tokens.text.body*4/3,
    fill="surfaceContainerHigh"},
  controls,body,nav,
  ui.Text{id="empty",position="absolute",left=T.wallpaper.content_inset,top=height/2-T.wallpaper.empty_center_offset,width=width-T.wallpaper.content_inset*2,height=T.wallpaper.empty_height,
    text=ui.bind("extras.empty_message"),visible=#rows==0 and not ui.resolve_for_check(ui.bind("extras.status_visible")),font_size=T.panel.empty_font,
    text_align="center",color=carousel and "onSurface" or T.wallpaper.photo_ink},
  ui.Text{id="status",position="absolute",left=T.wallpaper.content_inset,top=body_y+body_h/2+T.wallpaper.status_center_offset,width=width-T.wallpaper.content_inset*2,height=T.wallpaper.status_height,
    text=ui.bind("extras.status_message"),visible=ui.bind("extras.status_visible"),font_size=T.panel.empty_font,
    text_align="center",color="onSurfaceVariant"}}
if backdrop then table.insert(children,1,scrim);table.insert(children,1,backdrop) end
return kit.Panel{id="extras",edge="bottom",open=ui.bind("panel.open"),width=width,height=height,
  duration=T.panel.duration,transition=T.panel.transition,reveal=false,slide_distance=height,fade=false,radius=T.panel.radius,
  fill=carousel and "surface" or "transparent",border_width=carousel and T.panel.border or 0,
  border_color="outlineVariant",padding=0,gap=0,
  on_activate=ui.action("close"),aria_label="Close wallpaper picker",table.unpack(children)}
