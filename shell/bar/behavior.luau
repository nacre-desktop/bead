-- SPDX-License-Identifier: GPL-3.0-only
return {stores={},actions={},computed={
    ["bar.title.width"]=function(s) return math.min(550,#s["bead.focused_title"]*9.6) end,
}}
