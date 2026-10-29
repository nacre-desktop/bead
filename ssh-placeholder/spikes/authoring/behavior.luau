--!strict
export type App = { id: string, name: string, selected: boolean }
return {
    stores = { ["launcher.query"] = "", ["launcher.scroll"] = 0,
        ["launcher.selection"] = "", ["launcher.route"] = "home" },
    actions = {
        select = function(s, id) s["launcher.selection"] = id end,
        navigate = function(s, id) s["launcher.route"] = id end,
    },
    -- Model adapters supply the same typed rows to all three sample front ends.
    -- These names are declarations for the comparison, not a working model service.
    models = { ["launcher.apps"] = "App", ["launcher.navigation"] = "App" },
}
