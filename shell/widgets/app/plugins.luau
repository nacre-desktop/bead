-- SPDX-License-Identifier: GPL-3.0-only
-- Plugins page (spec 0203): installed plugins, their declared access, settings and sources.
-- The list comes from the plugin loader's generated index; every change is a `plugins.do` host action.
local context=require("../context")
local ui,kit=context.ui,context.kit
local spacing,text=kit.tokens.spacing,kit.tokens.text
local registry=require("../registry")
local settings_form=require("../settings-form")
local M={}

local TRUST={"Plugins run Luau code with the access they declare; the shell checks it where it controls access.",
    "Luau sandboxing is not a security boundary: only enable plugins you trust."}
local ACCESS={network="Network access",files="Read files",exec="Run programs"}

local function label(id,value,size,color,props)
    local p=props or {}
    p.id=id; p.text=value or ""; p.font_size=size or text.body*4/3; p.color=color or "onSurface"
    p.width=p.width or "fill"; p.height=p.height or 20
    return ui.Text(p)
end
local function button(id,value,action,props)
    local p=props or {}
    p.id=id; p.text=value; p.on_activate=action; p.focus_on_pointer=false
    p.height=p.height or spacing.huge+spacing.xs; p.shrink=0
    p.padding_left=spacing.md; p.padding_right=spacing.md
    p.width=p.width or math.ceil(ui.text_width(value,"IBM Plex Sans",text.body*4/3))+2*spacing.md
    return kit.Button(p)
end
-- Children may be nil (optional rows); a node's child list must not have holes.
local function node(constructor,props,...)
    for index=1,select("#",...) do
        local child=select(index,...)
        if child then table.insert(props,child) end
    end
    return constructor(props)
end
local function act(argument) return ui.action("plugins.do",argument) end
local function describe(capability)
    if ACCESS[capability] then return ACCESS[capability] end
    local kind,prefix=string.match(capability,"^(%a+):(.+)$")
    if kind=="topic" then return "Reads "..prefix.."*" end
    if kind=="action" then return "Triggers "..prefix.."*" end
    return capability
end
local function encode(value)
    if type(value)=="boolean" then return "b:"..tostring(value) end
    if type(value)=="number" then return "n:"..tostring(value) end
    return "s:"..tostring(value)
end

local function card(entry,navigate)
    local access={}
    for _,capability in entry.capabilities do table.insert(access,describe(capability)) end
    local caption=entry.version..(entry.author~="" and " · "..entry.author or "")
        ..(entry.origin=="bundled" and " · bundled" or entry.git and " · git" or " · local")
    local has_settings=next(entry.settings or {})~=nil
    return node(ui.Column,{id="plugin-"..entry.id,width="fill",shrink=0,gap=spacing.xs,padding=spacing.md,
        radius=kit.tokens.radii.medium,fill="surfaceContainer"},
        node(ui.Row,{id="plugin-heading",width="fill",height=spacing.huge+spacing.xs,align="center",gap=spacing.sm},
            label("plugin-name",entry.name,text.title*4/3,"onSurface",{width=0,grow=1,height=26}),
            entry.update and button("plugin-update","Update available",act("update|"..entry.id),
                {fill="tertiaryContainer",color="onTertiaryContainer"}) or nil,
            has_settings and entry.loaded and kit.IconButton{id="plugin-gear",text="settings",shrink=0,fill="transparent",
                aria_label="Plugin settings",focus_on_pointer=false,
                on_activate=navigate("select","@plugins/"..entry.id)} or nil,
            kit.Switch{id="plugin-enabled",checked=entry.enabled==true,aria_label="Enable "..entry.name,
                on_activate=act((entry.enabled and "disable|" or "enable|")..entry.id)}),
        label("plugin-meta",caption,text.caption*4/3,"onSurfaceVariant",{height=16}),
        entry.description~="" and label("plugin-description",entry.description,text.body*4/3,"onSurface") or nil,
        label("plugin-access",#access>0 and "Access: "..table.concat(access,", ") or "Access: none beyond drawing",
            text.caption*4/3,"onSurfaceVariant",{height=16}),
        entry.error~="" and label("plugin-error","Disabled after an error: "..entry.error,text.caption*4/3,"error",{height=16}) or nil)
end

local function settings_page(entry,navigate)
    local item={iid="plugin:"..entry.id,settings=entry.values}
    local package={settings=entry.settings,defaults=entry.values}
    local rows=settings_form.rows(item,package,function(kind,record)
        if kind~="setting" or record.value==nil then return nil end
        return act("set|"..entry.id.."|"..record.key.."|"..encode(record.value))
    end)
    return ui.Column{id="plugin-settings",width=0,grow=1,height="fill",gap=spacing.md,
        ui.Row{id="plugin-settings-heading",width="fill",height=54,shrink=0,gap=spacing.md,align="center",
            button("plugin-settings-back","Back",navigate("select","@plugins")),
            label("plugin-settings-title",entry.name.." settings",text.headline*4/3,"onSurface",{width=0,grow=1,height=28})},
        ui.Column{id="plugin-settings-card",width="fill",shrink=0,padding=spacing.md,radius=kit.tokens.radii.medium,
            fill="surfaceContainer",rows}}
end

function M.page(selected,navigate)
    local list=registry.plugins.list
    local open=string.match(selected,"^@plugins/(.+)$")
    for _,entry in list do
        if entry.id==open then return settings_page(entry,navigate) end
    end
    local message=ui.resolve_for_check(ui.bind("plugins.message"))
    if message==nil or message=="" then message=registry.plugins.notice end
    local cards={id="plugin-cards",width="fill",gap=spacing.md,shrink=0}
    for _,entry in list do table.insert(cards,card(entry,navigate)) end
    if #list==0 then table.insert(cards,label("plugins-empty","No plugins installed.",text.body*4/3,"onSurfaceVariant")) end
    return node(ui.Column,{id="plugins-page",width=0,grow=1,height="fill",gap=spacing.md},
        label("plugins-title","Plugins",text.headline*4/3,"onSurface",{height=28}),
        label("plugins-trust",TRUST[1],text.caption*4/3,"onSurfaceVariant",{height=16}),
        label("plugins-trust-boundary",TRUST[2],text.caption*4/3,"onSurfaceVariant",{height=16}),
        registry.plugins.user_dir and ui.Row{id="plugins-add",width="fill",height=spacing.huge+spacing.md,shrink=0,gap=spacing.sm,align="center",
            kit.TextInput{id="plugins-source",width=0,grow=1,height=spacing.huge+spacing.md,text=ui.bind("plugins.source"),
                placeholder="Git URL or absolute folder path",fill="surface",border_width=1,border_color="outlineVariant",
                aria_label="Plugin source"},
            button("plugins-add-button","Add",act("add")),
            button("plugins-check","Check for updates",act("check"),{fill="surfaceContainerHigh"})} or nil,
        message~="" and label("plugins-message",message,text.caption*4/3,"onSurfaceVariant",{height=18}) or nil,
        ui.Scroll{id="plugins-scroll",width="fill",grow=1,scroll=ui.bind("plugins.scroll"),ui.Column(cards)})
end

return M
