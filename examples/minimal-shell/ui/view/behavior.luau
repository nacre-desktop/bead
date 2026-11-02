-- SPDX-License-Identifier: MIT
return {
    stores={name="",count=0},
    actions={increment=function(s) s.count+=1 end},
    computed={
        greeting=function(s) return s.name=="" and "Your own shell, built on Bead" or "Welcome, "..s.name end,
        counter=function(s) return "Button presses: "..tostring(s.count) end,
    },
}
