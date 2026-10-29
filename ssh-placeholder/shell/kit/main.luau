-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
return kit.Panel{edge="left",id="kit",width=420,height=680,open=ui.bind("open"),padding=16,
 kit.Card{id="controls",gap=8,
  ui.Text{id="title",text="Framework components",font_size=24,height=32},
  kit.Button{id="button",text="Button",on_activate=ui.action("detail")},
  kit.IconButton{id="icon",text="star",on_activate=ui.action("detail")},
  kit.Switch{id="switch",checked=ui.bind("enabled"),on_activate=ui.action("toggle")},
  kit.Slider{id="slider",value=ui.bind("volume"),label="Volume",on_change="set_volume",width=320},
  kit.Tile{id="tile",text="Toggle and details",on_toggle=ui.action("toggle"),on_detail=ui.action("detail")},
  kit.SegmentedControl{id="segments",value=ui.bind("choice"),on_change="select",items={{id="quiet",label="Quiet"},{id="normal",label="Normal"}}},
  kit.LevelMeter{id="meter",value=.65,width=320},
  kit.TextInput{id="text",text=ui.bind("query"),height=40},
  kit.BlurredArtwork{id="artwork",path="sample.png",width=96,height=64,blur=6},
 },
 kit.List{id="list",scroll=ui.bind("scroll"),height=90,grow=0,
  ui.Column{id="rows",kit.ListRow{id="row",text="List row",on_activate=ui.action("detail")},kit.ListRow{id="row2",text="Second row",on_activate=ui.action("detail")}},
 },
 kit.Peek{id="peek",edge="left",visible=false,ui.Text{text="Peek"}},
 kit.Tooltip{id="tooltip",edge="left",visible=false,ui.Text{text="Tooltip"}},
}
