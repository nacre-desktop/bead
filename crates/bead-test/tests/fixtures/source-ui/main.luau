-- SPDX-License-Identifier: MIT
local ui=require("bead.ui")
return ui.Panel{id="source",width=320,height=100,padding=12,fill="surface",ui.Column{gap=4,
 ui.Text{text=ui.bind("title"),height=28,color="onSurface"},
 ui.Text{text="Reconnecting",visible=ui.bind("transport.reconnecting"),height=24,color="onSurfaceVariant"},
}}
