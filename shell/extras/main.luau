-- SPDX-License-Identifier: GPL-3.0-only
local ui = require("bead.ui")
local kit = require("bead.kit")
local T = require("./tokens")
local kind = ui.resolve_for_check(ui.bind("extras.kind")) or "overview"
local overview = kind == "overview"
local shortcuts = kind == "keybindings"
local screen_w = ui.resolve_for_check(ui.bind("bead.viewport.width")) or T.panel.overview_width
local screen_h = ui.resolve_for_check(ui.bind("bead.viewport.height")) or T.panel.default_screen_height
local width = overview and math.min(T.panel.overview_width, screen_w*T.panel.max_width_fraction) or T.panel.search_width
local configured_body = overview and math.min(T.panel.overview_body_max, math.max(T.panel.body_min, screen_h-T.panel.overview_body_reserve)) or T.panel.search_body_height
-- SearchSurface has 26px of vertical padding/gaps inside its nominal body height.
local body_height = configured_body-T.panel.body_internal_loss
local panel_height = configured_body+T.panel.search_chrome
local body_width = width-T.panel.horizontal_inset*2

local function row(item, selected)
  local row_h = shortcuts and T.shortcuts.row_height or T.palette.row_height
  local row_r = shortcuts and T.shortcuts.row_radius or T.palette.row_radius
  if shortcuts then
    return ui.Box{id="shortcut-"..tostring(item.id),width="fill",height=row_h,radius=row_r,fill="surfaceContainer",
      ui.Text{id="shortcut-key",position="absolute",left=T.shortcuts.padding,top=0,width=T.shortcuts.key_width,height=row_h,
        text=item.key or "",font_size=T.shortcuts.font,color="primary",family="JetBrains Mono NF"},
      ui.Text{id="shortcut-description",position="absolute",left=T.shortcuts.description_offset,top=0,
        width=body_width-T.shortcuts.description_offset-T.shortcuts.padding,height=row_h,text=item.description or "",
        font_size=T.shortcuts.font,color="onSurface"}}
  end
  local activate = item.disabled ~= true and ui.action("extras.do","activate:"..tostring(item.id)) or nil
  return ui.Button{id="palette-"..tostring(item.id),width="fill",height=row_h,radius=row_r,padding=T.palette.padding,
    fill=selected and "secondaryContainer" or "surfaceContainer",hover_fill="secondaryContainer",activate_on_release=true,focus_on_pointer=false,on_activate=activate,
    aria_label=(item.name or "").." "..(item.description or ""),
    ui.Row{width="fill",height=row_h-T.palette.padding*2,gap=T.palette.content_gap,align="center",
      ui.Text{text=item.kind == "app" and "apps" or item.icon or "apps",family="Material Symbols Rounded",width=T.palette.icon_size,height=T.palette.icon_size,
        font_size=T.palette.icon_font,color="primary"},
      ui.Column{grow=1,gap=T.palette.description_gap,
        ui.Text{id="palette-name",text=item.name or "",height=T.palette.name_height,font_size=T.palette.name_font,color="onSurface"},
        ui.Text{id="palette-description",text=item.description or "",height=T.palette.description_height,font_size=T.palette.description_font,color="onSurfaceVariant"}}}}
end

local function window_tile(item)
  local selected = item.selected == true
  return ui.Button{id="window-"..tostring(item.id),width=item.tile_width,height=item.tile_height,
    radius=T.overview.thumbnail_radius,padding=T.overview.tile_padding,
    fill="transparent",hover_fill="transparent",
    activate_on_release=true,focus_on_pointer=false,on_activate=ui.action("extras.do","focus:"..tostring(item.id)),aria_label=item.title or item.app_id or "Window",
    ui.Box{id="preview",position="absolute",left=0,top=0,width=item.tile_width,height=item.preview_height,
      radius=T.overview.thumbnail_radius,fill=selected and "secondaryContainer" or "surfaceContainerHigh",
      ui.Image{position="absolute",left=(item.tile_width-T.overview.icon_size)/2,top=(item.preview_height-T.overview.icon_size)/2,
        icon=item.icon or "application-x-executable",width=T.overview.icon_size,height=T.overview.icon_size,
        aria_label=item.title or "Window"}},
    ui.Text{id="window-title",position="absolute",left=0,top=item.preview_height+T.overview.title_gap,
      width=item.tile_width,height=T.overview.title_height,text=item.title or item.app_id or "Window",
      font_size=T.overview.title_font,color="onSurface",text_align="left"}}
end

local function workspace_card(group)
  local ws = group.workspace
  local count = #group.windows
  local columns = count == 1 and 1 or T.overview.window_columns
  local rows = math.ceil(count/columns)
  local tile_h = count <= 2 and T.overview.two_window_cell_height or T.overview.multi_window_cell_height
  local tile_extent = math.max(T.overview.thumbnail_height, rows*tile_h)
  local tiles = {}
  for i, item in ipairs(group.windows) do
    item.tile_width = count == 1 and T.overview.single_tile_width or T.overview.multi_tile_width
    item.tile_height = tile_h-T.overview.tile_cell_inset
    item.preview_height = item.tile_height-T.overview.title_height-T.overview.title_gap
    item.selected = item.selected == true
    tiles[i] = window_tile(item)
  end
  local kids = {
    ui.Button{id="workspace-title",position="absolute",left=T.overview.workspace_title_left,top=T.overview.workspace_title_top,width=T.overview.workspace_title_width,
      height=T.overview.workspace_title_height,padding=0,radius=T.overview.workspace_title_radius,fill="transparent",
      activate_on_release=true,focus_on_pointer=false,on_activate=ui.action("extras.do","workspace:"..tostring(ws.index)),aria_label=ws.name or ("Workspace "..tostring(ws.index)),
      ui.Row{width="fill",height=T.overview.workspace_title_height,gap=T.overview.title_icon_gap,align="center",
        ui.Text{text="workspaces",family="Material Symbols Rounded",font_size=T.overview.workspace_icon_font,
          color="primary",width=T.overview.workspace_icon_width,height=T.overview.workspace_title_height},
        ui.Text{text=tostring(ws.index).." · "..((ws.name and ws.name~="") and ws.name or "Workspace"),
          height=T.overview.workspace_title_height,font_size=T.overview.workspace_title_font,color="primary"}}},
    ui.Scroll{id="window-grid-extent",position="absolute",left=T.overview.tile_left,top=T.overview.tile_top,
      scroll=ui.bind("extras.window_scroll."..tostring(math.clamp(ws.index,1,64))),width=T.overview.tile_area_width,height=T.overview.thumbnail_height,
      ui.Grid{id="window-grid",width=T.overview.tile_area_width,height=tile_extent,
        columns=columns,row_height=tile_h-T.overview.tile_cell_inset,
        gap_x=T.overview.window_gap_x,gap_y=T.overview.window_gap_y,
        table.unpack(tiles)}},
  }
  if count == 0 then
    kids[#kids+1] = ui.Text{id="workspace-empty",position="absolute",left=0,top=T.overview.empty_top,
      width=T.overview.tile_area_width,height=T.overview.empty_height,text="Empty",
      font_size=T.overview.empty_font,text_align="center",color="onSurfaceVariant"}
  end
  return ui.Box{id="workspace-card-"..tostring(ws.id),width=T.overview.workspace_width,height=T.overview.workspace_height,
    radius=T.overview.workspace_radius,
    fill="surfaceContainer",table.unpack(kids)}
end

local body
if overview then
  local groups = ui.resolve_for_check(ui.bind("extras.groups")) or {}
  local columns = math.max(T.overview.min_columns,math.min(T.overview.max_columns,
    math.floor((body_width+T.overview.workspace_gap)/(T.overview.workspace_width+T.overview.workspace_gap))))
  local count = #groups
  local rows = math.ceil(count/columns)
  local extent = math.max(body_height,rows*T.overview.workspace_height+math.max(0,rows-1)*T.overview.workspace_gap)
  body = ui.Scroll{id="overview-scroll",scroll=ui.bind("extras.scroll"),width="fill",height=body_height,
    ui.Box{id="overview-extent",width=body_width,height=extent,shrink=0,
      ui.Grid{id="workspace-grid",position="absolute",left=0,top=0,width=body_width,
        columns=columns,row_height=T.overview.workspace_height,gap_x=T.overview.workspace_gap,gap_y=T.overview.workspace_gap,
        ui.Repeat{id="workspace-groups",model=ui.bind("extras.groups"),key="id",
          render=workspace_card}}}}
else
  body = ui.Scroll{id="extras-scroll",scroll=ui.bind("extras.scroll"),width="fill",height=body_height,
    ui.Column{id="extras-rows",width="fill",gap=shortcuts and T.shortcuts.row_gap or T.palette.row_gap,
      ui.Repeat{id="rows",model=ui.bind("extras.filtered"),key="id",render=function(item)
        return row(item,item.selected == true)
      end}}}
end

local heading = {
  ui.Text{id="title",text=ui.bind("extras.title"),width=0,grow=1,height=T.panel.heading_height,font_size=T.panel.heading_font,color="primary"},
}
if shortcuts then heading[#heading+1] = kit.IconButton{id="refresh",aria_label="Refresh shortcuts",text="refresh",width=T.panel.refresh_size,height=T.panel.refresh_size,padding=0,
  activate_on_release=true,on_activate=ui.action("extras.do","refresh")} end
heading[#heading+1] = kit.Button{id="close",text="Close",aria_label="Close",width=T.panel.close_width,height=T.panel.close_height,padding=0,font_size=T.panel.close_font,
  activate_on_release=true,on_activate=ui.action("extras.close")}

return kit.Panel{id="extras",edge="top",open=ui.bind("panel.open"),width=width,height=panel_height,
  duration=T.panel.duration,transition="ease",fade=false,radius=T.panel.radius,fill="surface",border_width=T.panel.border,border_color="outlineVariant",
  padding_left=T.panel.horizontal_inset,padding_right=T.panel.horizontal_inset,
  padding_top=T.panel.heading_top,padding_bottom=T.panel.bottom_inset,gap=T.panel.search_gap,
  ui.Row{id="heading",width="fill",height=T.panel.heading_height,gap=T.panel.heading_gap,align="center",table.unpack(heading)},
  kit.TextInput{id="search",text=ui.bind("extras.query"),placeholder=ui.bind("extras.placeholder"),width="fill",
    height=T.panel.search_height,padding_left=T.panel.search_padding,padding_right=T.panel.search_padding,padding_top=T.panel.search_vertical,padding_bottom=T.panel.search_vertical,radius=T.panel.search_radius,font_size=kit.tokens.text.body*4/3,
    fill="surfaceContainerHigh"},
  ui.Box{id="body",width="fill",height=body_height,body},
  ui.Text{id="empty-message",position="absolute",left=T.panel.horizontal_inset,
    top=T.panel.empty_top+body_height/2-T.panel.empty_height/2,width=body_width,height=T.panel.empty_height,
    text=ui.bind("extras.empty_message"),visible=shortcuts and ui.bind("extras.empty") or false,font_size=T.panel.empty_font,
    text_align="center",color="onSurfaceVariant"},
  ui.Text{id="status-message",position="absolute",left=T.panel.horizontal_inset,
    top=T.panel.empty_top+body_height/2-T.panel.empty_height/2,width=body_width,height=T.panel.empty_height,
    text=ui.bind("extras.status_message"),visible=ui.bind("extras.status_visible"),font_size=T.panel.empty_font,
    text_align="center",color="onSurfaceVariant"},
}
