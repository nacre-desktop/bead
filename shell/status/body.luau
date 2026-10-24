-- SPDX-License-Identifier: GPL-3.0-only
-- The same content builder can be a standalone panel or a dock-owned column.
local ui=require("bead.ui")
local kit=require("bead.kit")
return function(embedded,props)
    if not embedded then
        props.reveal=true;props.fade=false;props.transition="ease-in-out"
        return kit.Panel(props)
    end
    props.edge=nil;props.open=nil;props.duration=nil;props.radius=nil;props.fill=nil
    return ui.Column(props),props.height
end
