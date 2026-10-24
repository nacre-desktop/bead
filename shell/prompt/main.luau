-- SPDX-License-Identifier: GPL-3.0-only
-- Selector prompt (B0093): `nacre-menu pick|input`, the Wi-Fi chooser and the several-windows chooser.
-- The launcher's look in QML's picker layout: a title, a filtered list, a hint and a filter field (pick), or a
-- title and one text field with Cancel / Continue (input). Geometry is in tokens.luau.
local ui=require("bead.ui")
local kit=require("bead.kit")
local VirtualList=require("../kit/virtual-list")
local T=require("./tokens")
local P,L,E,F=T.panel,T.list,T.empty,T.form
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local input=get("prompt.kind")=="input"
local selected_id=get("prompt.current")

local function row(item)
  local on=item.id==selected_id
  local disabled=item.disabled==true
  return ui.Button{id="choice",width="fill",height=L.row_height,radius=L.row_radius,padding=0,fill="transparent",
    selected=on,selected_fill=ui.tint("surface","onSurface",L.selected_tint),hover_fill=disabled and "transparent" or ui.tint("surface","onSurface",L.hover_tint),
    activate_on_release=true,focus_on_pointer=false,on_activate=(not disabled) and ui.action("prompt.do","choose:"..item.id) or nil,aria_label=item.name,
    ui.Box{position="absolute",left=0,top=L.accent_top,width=L.accent_width,height=L.row_height-L.accent_inset,radius=L.accent_width/2,fill="primary",visible=on},
    ui.Text{position="absolute",left=L.icon_left,top=(L.row_height-L.icon_size)/2,width=L.icon_size,height=L.icon_size,text=item.icon,
      family="Material Symbols Rounded",font_size=L.icon_font,text_align="center",color="onSurface"},
    ui.Text{position="absolute",left=L.text_left,top=L.name_top,width=L.width-L.text_left-100,height=L.name_height,text=item.name,
      font_size=L.name_font,color=disabled and "onSurfaceVariant" or "onSurface"},
    ui.Text{position="absolute",left=L.text_left,top=L.detail_top,width=L.width-L.text_left-32,height=L.detail_height,text=item.description,
      font_size=L.detail_font,color="onSurfaceVariant"}}
end

local function text_button(id,label,left,primary,action)
  local width=math.floor(utf8.len(label)*F.char_width+2*F.button_pad)
  return kit.Button{id=id,position="absolute",left=left,top=P.height-F.footer_bottom-F.button_height,width=width,height=F.button_height,padding=0,
    radius=F.button_radius,fill=primary and "primary" or "surfaceContainerHigh",aria_label=label,on_activate=action,
    ui.Text{position="absolute",left=0,top=(F.button_height-20)/2,width=width,height=20,text=label,font_size=F.button_font,text_align="center",
      color=primary and "onPrimary" or "onSurface"}},width
end

local kids={
  ui.Text{id="title",position="absolute",left=T.title.left,top=T.title.top,width=P.width-2*T.title.left,height=T.title.height,
    text=ui.bind("prompt.title"),font_size=T.title.font,color="onSurfaceVariant"},
}
local function add(node) kids[#kids+1]=node end

if input then
  local left=(P.width-F.width)/2
  local top=F.top
  add(ui.Text{id="form-title",position="absolute",left=left,top=top,width=F.width,height=F.title_height,text=ui.bind("prompt.title"),font_size=F.title_font,color="onSurface"})
  top=top+F.title_height+F.gap
  add(ui.Text{id="label",position="absolute",left=left,top=top,width=F.width,height=F.label_height,text=ui.tr("bead.prompt.value"),font_size=F.label_font,color="onSurface"})
  top=top+F.label_height+F.label_gap
  add(kit.TextInput{id="value",position="absolute",left=left,top=top,width=F.width,height=F.input_height,text=ui.bind("prompt.input"),
    padding=F.input_padding,radius=F.input_radius,font_size=F.input_font,fill="surfaceContainer"})
  local error=get("prompt.error")
  if error~="" then
    add(ui.Text{id="error",position="absolute",left=left,top=P.height-F.footer_bottom-F.button_height-F.button_gap-F.error_height,width=F.width,
      height=F.error_height,text=error,font_size=T.hint.font,color="error"})
  end
  local cancel,width=text_button("cancel",ui.tr("bead.prompt.cancel"),left,false,ui.action("prompt.do","cancel"))
  add(cancel)
  add((text_button("submit",ui.tr("bead.prompt.continue"),left+width+F.button_gap,true,ui.action("prompt.do","submit"))))
else
  add(ui.Box{id="list",position="absolute",left=L.left,top=L.top,width=L.width,height=L.height,
    VirtualList{id="choices",scroll=ui.bind("prompt.scroll"),width=L.width,height=L.height,
      gap=L.row_gap,row_height=L.row_height,model=ui.bind("prompt.matches"),key="id",render=row}})
  add(ui.Text{id="empty",position="absolute",left=E.left,top=E.top,width=E.width,height=E.height,text=ui.tr("bead.prompt.no-matches"),
    visible=ui.bind("prompt.empty"),font_size=E.font,color="onSurfaceVariant"})
  add(ui.Text{id="hint",position="absolute",left=T.hint.left,top=P.height-T.hint.bottom,width=T.hint.width,height=T.hint.height,
    text=ui.tr("bead.prompt.hint"),font_size=T.hint.font,color="onSurfaceVariant"})
  add(kit.TextInput{id="search",position="absolute",left=T.search.left,top=P.height-T.search.bottom,width=P.width-2*T.search.inset,
    height=T.search.height,text=ui.bind("prompt.query"),placeholder=ui.tr("bead.prompt.filter"),padding=T.search.padding,
    radius=T.search.radius,font_size=T.search.font})
end

return kit.Panel{id="prompt",edge=P.edge,open=ui.bind("panel.open"),width=P.width,height=P.height,max_width="90%",max_height="90%",
  transition=P.transition,duration=P.duration,radius=P.radius,fill="surface",border_width=P.border,border_color="outlineVariant",padding=0,
  table.unpack(kids)}
