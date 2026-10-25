-- SPDX-License-Identifier: GPL-3.0-only
-- The rejected list editor is not a user view. Keep this behavior-backed empty
-- entry for headless transactions until the Widgets app owns composition.
local ui=require("bead.ui")
return ui.Group{}
