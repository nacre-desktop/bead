-- SPDX-License-Identifier: GPL-3.0-only
-- Internal transaction fixture. No public route opens this behavior host.
local c=require("../context")
local registry=require("../registry")
local ui=c.ui
local width=ui.resolve_for_check(ui.bind("bead.viewport.width"))
local height=ui.resolve_for_check(ui.bind("bead.viewport.height"))
local draft=ui.resolve_for_check(ui.bind("widgets.draft"))
local saved=ui.resolve_for_check(ui.bind("widgets.layout"))
local document=draft[1] or saved[1] or registry.default(width)
registry.validate(document)
return ui.Box{id="widget-transactions",width=1,height=1,
    metadata=require("../editing").seed(document,width,height)}
