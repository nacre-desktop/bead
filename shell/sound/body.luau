-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
return function(options)
local embedded=options~=nil
local panel=require("../status/body")
local T=kit.tokens
local padding=embedded and 0 or T.spacing.panel
local content_width=embedded and options.width or T.spacing.huge*11
local width=content_width+padding*2
local sources=ui.resolve_for_check(ui.bind("cc.src")) or {}
local function source(id)
 for _,item in ipairs(sources) do if item.id==id then return item end end
 return {available=false,value=0,status=""}
end
local devices=ui.resolve_for_check(ui.bind("cc.audio_devices")) or {}
local ROW=T.spacing.huge+T.spacing.xxl
-- B0095: QML places text by its line box (about 1.295 times the font size, top aligned), so centres are
-- worked out from it; the section headings are one 12 pt line high.
local function ytop(center,px) return center-px*.6475 end
local HEAD=12*4/3*1.295
local ICON=(T.icons.large-1)*4/3
-- QML's caption is as wide as its text and the meter follows it; Bead sets the box, so its width is estimated.
local caption_label=ui.tr("bead.panel.audio-input-level")
local caption_width=math.ceil((utf8.len(caption_label) or #caption_label)*(T.text.caption-2)*4/3*.45)
local function picker(kind)
 local items={}
 for _,item in ipairs(devices) do
  if item.kind==kind then
   local ink=item.selected and "onPrimaryContainer" or "onSurface"
   items[#items+1]=kit.Button{id=item.id,width=content_width,height=44,padding=0,radius=T.spacing.xl,
    fill=item.selected and "primaryContainer" or "surfaceContainer",
    on_activate=item.enabled and ui.action("cc.do","choose:audio."..kind..":"..item.id) or nil,
    ui.Text{position="absolute",left=14,top=ytop(22,ICON),width=24,height=ICON*1.295,text=kind=="input" and "mic" or "speaker",
     family="Material Symbols Rounded",font_size=ICON,color=ink},
    ui.Text{position="absolute",left=44,top=ytop(22,(T.text.caption+1)*4/3),width=content_width-80,height=24,text=item.label,font_size=(T.text.caption+1)*4/3,color=ink},
    ui.Text{position="absolute",left=content_width-34,top=ytop(22,T.icons.medium*4/3),width=20,height=T.icons.medium*4/3*1.295,text=item.selected and "check" or "",
     family="Material Symbols Rounded",font_size=T.icons.medium*4/3,color=ink}}
  end
 end
 return ui.Column{id=kind.."-devices",width=content_width,gap=T.spacing.xs,table.unpack(items)},#items
end
local output_devices,output_count=picker("output")
local input_devices,input_count=picker("input")
local function level(id,label)
 local item=source(id)
 return ui.Box{id=id,width=content_width,height=ROW,radius=T.spacing.xl,fill="surfaceContainer",
  ui.Text{id="label",position="absolute",left=46,top=T.spacing.sm,width=content_width-60,height=18,text=label,
   font_size=(T.text.caption-1)*4/3,color="onSurfaceVariant"},
  kit.IconButton{id="mute",position="absolute",left=T.spacing.sm,top=22,width=32,height=32,padding=0,fill="transparent",font_size=ICON,text_align="center",
   text=id=="audio.input" and (item.on and "mic_off" or "mic") or (item.on and "volume_off" or "volume_up"),
   aria_label=label,on_activate=item.toggle and ui.action("cc.do","toggle:"..id) or nil,color="primary"},
  kit.Slider{id="level",position="absolute",left=42,top=24,width=content_width-104,value=item.value,label=label,
   enabled=item.set==true,on_change=item.set and "cc.do" or nil,change_arg=function(n) return "set:"..id..":"..tostring(n) end,pointer_format={prefix="set:"..id..":",precision=3}},
  ui.Text{id="value",position="absolute",left=content_width-56,top=32,width=42,height=20,text=item.available and item.status or "—",
   text_align="right",font_size=T.text.caption*4/3,color="onSurfaceVariant"}}
end
-- Application streams (B0086, N0061): an icon (or the apps glyph), name, volume slider with its
-- percent and a mute button on one card, as the QML card. The slider is perceptual like a device's (Quickshell's
-- volume is the cube root of PipeWire's), the call linear.
local streams=ui.resolve_for_check(ui.bind("cc.streams")) or {}
local function stream_card(item)
 local mute_size=32
 local mute_left=content_width-10-mute_size
 return ui.Box{id=item.id,width=content_width,height=64,radius=T.radii.large,fill=ui.alpha("surfaceContainer",.72),
  item.icon~="" and ui.Image{id="icon",position="absolute",left=14,top=13,width=20,height=20,icon=item.icon}
   or ui.Text{id="icon",position="absolute",left=14,top=13,width=24,height=24,text="apps",
    family="Material Symbols Rounded",font_size=T.icons.medium*4/3,color="onSurfaceVariant"},
  ui.Text{id="name",position="absolute",left=44,top=12,width=content_width-96,height=20,text=item.label,
   font_size=(T.text.caption+1)*4/3,color="onSurface"},
  kit.Slider{id="volume",position="absolute",left=T.spacing.sm,top=32,width=content_width-104,value=item.value,
   label=ui.tr("bead.panel.audio-app-volume",{name=item.label}),enabled=item.enabled==true,
   on_change=item.enabled and "cc.do" or nil,change_arg=function(n) return "set:stream:"..item.id..":"..tostring(n) end,pointer_format={prefix="set:stream:"..item.id..":",precision=3}},
  ui.Text{id="percent",position="absolute",left=mute_left+mute_size-42-40,top=38,width=40,height=18,text=item.status,
   text_align="right",font_size=(T.text.caption-1)*4/3,color="onSurfaceVariant"},
  kit.IconButton{id="mute",position="absolute",left=mute_left,top=(64-mute_size)/2,width=mute_size,height=mute_size,padding=0,
   fill="transparent",font_size=ICON,text_align="center",text=item.muted and "volume_off" or "volume_up",
   aria_label=ui.tr("bead.panel.audio-app-mute",{name=item.label}),
   on_activate=item.enabled and ui.action("cc.do","toggle:stream:"..item.id) or nil}}
end
local app_cards={}
for _,item in ipairs(streams) do app_cards[#app_cards+1]=stream_card(item) end
local LEVEL_ROW=15.7
local function devices_height(count) return count>0 and count*44+(count-1)*T.spacing.xs+T.spacing.xs or 0 end
local output_height=HEAD+T.spacing.xs+devices_height(output_count)+ROW
local input_height=HEAD+T.spacing.xs+devices_height(input_count)+ROW+T.spacing.xs+LEVEL_ROW
local stream_height=#streams==0 and 36 or #streams*64+(#streams-1)*T.spacing.xs
local apps_height=HEAD+T.spacing.xs+stream_height
local height=padding*2+output_height+input_height+apps_height+T.spacing.md*2
return panel(embedded,{id="sound",edge="top",open=ui.bind("panel.open"),duration=ui.bind("view.duration"),width=width,height=height,padding=padding,gap=T.spacing.md,
 ui.Column{id="output",width=content_width,height=output_height,gap=T.spacing.xs,
  ui.Text{id="output-heading",text=ui.tr("bead.dashboard.audio-output"),width=content_width,height=HEAD,font_size=T.text.body*4/3,font_weight=500,color="onSurface"},
  output_devices,
  level("audio.output",ui.resolve_for_check(ui.bind("status.device"))),
 },
 ui.Column{id="input",width=content_width,height=input_height,gap=T.spacing.xs,
  ui.Text{id="input-heading",text=ui.tr("bead.dashboard.audio-input"),width=content_width,height=HEAD,font_size=T.text.body*4/3,font_weight=500,color="onSurface"},
  input_devices,
  level("audio.input",ui.resolve_for_check(ui.bind("audio.input_device"))),
  -- Live microphone level: only while the default input is live (the daemon captures while it is shown).
  ui.Row{id="input-level",width=content_width,height=LEVEL_ROW,gap=T.spacing.md,align="center",
   ui.Text{id="caption",text=caption_label,width=caption_width,shrink=0,height=LEVEL_ROW,
    font_size=(T.text.caption-2)*4/3,color="onSurfaceVariant"},
   kit.LevelMeter{id="meter",width=math.max(0,content_width-100),height=5,label=ui.tr("bead.panel.audio-input-level"),
    track="surfaceContainer",value=ui.resolve_for_check(ui.bind("audio.peak_on")) and ui.resolve_for_check(ui.bind("audio.peak")) or 0}},
 },
 ui.Column{id="applications",width=content_width,height=apps_height,gap=T.spacing.xs,
  ui.Text{id="apps-heading",text=ui.tr("bead.panel.audio-applications"),width=content_width,height=HEAD,font_size=T.text.body*4/3,font_weight=500,color="onSurface"},
  if #streams==0 then
   ui.Text{id="apps-empty",text=ui.tr("bead.panel.audio-apps-empty"),width=content_width,height=36,font_size=T.text.caption*4/3,color="onSurfaceVariant"}
  else
   ui.Column{id="apps",width=content_width,gap=T.spacing.xs,table.unpack(app_cards)}
 },
})
end
