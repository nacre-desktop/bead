-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local padding=T.spacing.panel
local content_width=T.spacing.huge*11
local width=content_width+padding*2
local streams=ui.resolve_for_check(ui.bind("audio.streams")) or {}
local muted=ui.resolve_for_check(ui.bind("status.active")) == true
local output_height=24+20+48+T.spacing.xs*2
local input_height=20+48+T.spacing.xs
local stream_height=#streams==0 and 36 or #streams*64+(#streams-1)*T.spacing.xs
local apps_height=20+stream_height+T.spacing.xs
local height=padding*2+output_height+input_height+apps_height+T.spacing.md*2
return kit.Panel{id="sound",edge="top",open=ui.bind("panel.open"),duration=ui.bind("view.duration"),width=width,height=height,padding=padding,gap=T.spacing.md,
 ui.Column{id="output",width=content_width,height=output_height,gap=T.spacing.xs,
  ui.Text{id="output-heading",text=ui.tr("bead.dashboard.audio-output"),width=content_width,height=24,font_size=T.text.title*4/3,color="onSurface"},
  ui.Text{id="device",width=content_width,text=ui.bind("status.device"),height=20,font_size=T.text.caption*4/3,color="onSurfaceVariant"},
  ui.Box{id="volume-control",width=content_width,height=48,radius=T.radii.large,fill="surfaceContainer",
   ui.Row{width=content_width-T.spacing.md*2,height=48,gap=T.spacing.sm,align="center",
    ui.Text{id="volume-icon",text=muted and "volume_off" or "volume_up",family="Material Symbols Rounded",width=T.icons.medium*4/3,height=24,font_size=T.icons.medium*4/3,color="onSurfaceVariant"},
    ui.Text{id="volume",text=ui.bind("status.primary"),height=28,grow=1,font_size=T.text.body*4/3,color="onSurface"},
    ui.Text{id="mute",text=ui.bind("status.mute"),height=20,font_size=T.text.caption*4/3,color="onSurfaceVariant"},
   },
  },
 },
 ui.Column{id="input",width=content_width,height=input_height,gap=T.spacing.xs,
  ui.Text{id="input-heading",text=ui.tr("bead.dashboard.audio-input"),width=content_width,height=20,font_size=T.text.title*4/3,color="onSurface"},
  ui.Box{id="input-control",width=content_width,height=48,radius=T.radii.large,fill="surfaceContainer",
   ui.Row{width=content_width-T.spacing.md*2,height=48,gap=T.spacing.sm,align="center",
    ui.Text{id="input-icon",text="mic",family="Material Symbols Rounded",width=T.icons.medium*4/3,height=24,font_size=T.icons.medium*4/3,color="onSurfaceVariant"},
    ui.Text{id="input-device",text=ui.bind("audio.input_device"),height=28,grow=1,font_size=T.text.caption*4/3,color="onSurfaceVariant"},
   },
  },
 },
 ui.Column{id="applications",width=content_width,height=apps_height,gap=T.spacing.xs,
  ui.Text{id="apps-heading",text=ui.tr("bead.panel.audio-applications"),width=content_width,height=20,font_size=T.text.title*4/3,color="onSurface"},
  if #streams==0 then
   ui.Text{id="apps-empty",text=ui.tr("bead.panel.audio-apps-empty"),width=content_width,height=36,font_size=T.text.caption*4/3,color="onSurfaceVariant"}
  else
   ui.Repeat{id="apps",model=ui.bind("audio.streams"),key="id",render=function(item)
    return ui.Box{id="app",width=content_width,height=64,radius=T.radii.large,fill="surfaceContainer",
     ui.Text{id="app-name",text=item.label,position="absolute",left=T.spacing.md,top=T.spacing.lg,
      width=content_width-T.spacing.md*2,height=28,font_size=T.text.body*4/3,color="onSurface"}}
   end}
  ,
 },
}
