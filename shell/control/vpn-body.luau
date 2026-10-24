-- SPDX-License-Identifier: GPL-3.0-only
-- VPN dock page (B0087): the QML NacreControlVpn on kit components. A hint, one tile per saved profile
-- (connected = filled, press to switch) and one status line. The host projects the rows (`cc.vpn`) and the
-- line (`cc.vpn_message`); every size comes from the tile registry and the size-scale tokens.
local ui=require("bead.ui")
local kit=require("bead.kit")
local reg=require("./tiles")
local T=kit.tokens
return function(options)
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local width=options.width
local TEXT=(T.text.body-1)*4/3 -- the QML hint and status line are 11 pt
local gap=T.spacing.lg
-- The QML detail column keeps the (empty) section description above this page: its line plus the 20 px spacing.
local QML_DESCRIPTION=T.spacing.xl+T.spacing.md
local row_height=reg.layout.sizes.half.height
local kids,total={},0
local function add(node,height)
    table.insert(kids,node)
    total=total+height+(#kids>1 and gap or 0)
end
add(ui.Box{id="description",width=width,height=QML_DESCRIPTION,shrink=0},QML_DESCRIPTION)
local hint=kit.Paragraph{id="hint",text=ui.tr("bead.panel.control-vpn-hint"),width=width,font_size=TEXT,advance=.5,color="onSurfaceVariant"}
add(hint,hint.props.height)
for _,item in ipairs(get("cc.vpn")) do
    add(kit.ToggleTile{id="vpn-"..item.id,width=width,height=row_height,icon="vpn_key",label=item.label,detail=item.detail,on=item.on==true,
        available=item.enabled==true,on_activate=ui.action("cc.do","switch:vpn.state:"..item.id)},row_height)
end
local message=get("cc.vpn_message")
if message~="" then
    local note=kit.Paragraph{id="message",text=message,width=width,font_size=TEXT,advance=.5,color="onSurfaceVariant"}
    add(note,note.props.height)
end
kids.id="vpn"
kids.width=width
kids.height=total
kids.gap=gap
return ui.Column(kids),total
end
