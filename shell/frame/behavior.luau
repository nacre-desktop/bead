-- SPDX-License-Identifier: GPL-3.0-only
-- Edge lips of the desktop frame. A lip is a hover/click target on a screen edge, centred
-- along it, that opens the named panel view; its thickness is that edge's frame width.
-- Add, remove or retarget lips here; nothing else needs rebuilding.
--   id      unique name              edge    top | bottom | left | right
--   length  pixels along the edge    opens   panel view name (a lip whose view is not
--                                            registered, or is disabled in settings, is not shown)
-- `frame.lips.blocked_by` lists open panels during which hovering opens nothing.
return {stores={
    ["frame.lips"]={
        {id="dashboard",edge="top",length=208,opens="dashboard-today"},
        {id="assistant",edge="left",length=144,opens="assistant"},
        {id="controls",edge="right",length=144,opens="control"},
    },
    ["frame.lips.blocked_by"]={{id="launcher"},{id="session"}},
}}
