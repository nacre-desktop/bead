-- SPDX-License-Identifier: MIT
return {
    stores = { name = "", count = 0 },
    actions = { increment = function(s) s.count += 1 end },
    computed = { counter = function(s) return "Button presses: " .. tostring(s.count) end },
}
