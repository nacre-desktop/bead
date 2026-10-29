-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local width=get("view.width")
local padding,gap=T.spacing.panel,T.spacing.compact
local content=width-padding*2
local title,body,caption=T.text.title*4/3,T.text.body*4/3,T.text.caption*4/3
local title_height,caption_height=title*1.4,caption*1.4
local button_height=T.spacing.xxl+T.spacing.md
local kids={}
local height=padding*2
local function add(node,h)
    if #kids>0 then height=height+gap end
    height=height+h
    table.insert(kids,node)
end
local header_height=title_height+caption_height+T.spacing.xs
local heading_width=content-T.spacing.huge-T.spacing.md
add(ui.Row{id="header",height=header_height,width=content,gap=T.spacing.md,align="center",
    ui.Text{id="icon",text="package_2",family="Material Symbols Rounded",font_size=T.icons.large*4/3,
        width=T.spacing.huge,height=T.spacing.huge,shrink=0,color=get("status.ready") and "primary" or "onSurfaceVariant"},
    ui.Column{id="heading",height=header_height,width=heading_width,gap=T.spacing.xs,
        ui.Text{id="summary",text=ui.bind("status.updates_summary"),width=heading_width,font_size=title,height=title_height,color="onSurface"},
        ui.Text{id="checked",text=ui.bind("status.updates_checked"),width=heading_width,font_size=caption,height=caption_height,color="onSurfaceVariant"},
    },
},header_height)
local health=get("status.updates_health")
if health~="" then
    local note=kit.Paragraph{id="health",text=health,width=content,font_size=caption,color="onSurfaceVariant"}
    add(note,note.props.height)
end
-- Older producers omit packages entirely; the count-only dropdown stays compact.
local model=ui.bind("status.updates_packages")
local count=ui.model_count(model)
if count>0 then
    local row_height=T.spacing.xxl*2+T.spacing.md
    local list_height=math.min(count*(row_height+T.spacing.xs)-T.spacing.xs,T.spacing.huge*7)
    add(require("../kit/virtual-list"){id="packages",model=model,key="id",scroll=ui.bind("status.updates_scroll"),
        width=content,height=list_height,row_height=row_height,gap=T.spacing.xs,render=function(item)
            local badge=item.repo~="" and math.min(content/3,ui.text_width(item.repo,"IBM Plex Sans",caption)+T.spacing.md) or 0
            local inner=content-T.spacing.sm*2
            return ui.Column{id="package",width=content,height=row_height,padding=T.spacing.sm,gap=0,shrink=0,
                radius=T.radii.small,fill="surfaceContainer",
                ui.Row{id="name-row",width=inner,height=body*1.4,gap=T.spacing.sm,align="center",
                    ui.Text{id="name",text=item.name,width=inner-badge-(badge>0 and T.spacing.sm or 0),height=body*1.4,shrink=0,font_size=body,color="onSurface"},
                    item.repo~="" and ui.Text{id="repo",text=item.repo,width=badge,height=caption*1.4,padding_left=T.spacing.xs,shrink=0,
                        radius=T.radii.small,fill="surfaceContainerHigh",font_size=caption,color="onSurfaceVariant"} or nil,
                },
                ui.Text{id="versions",text=item.old.." → "..item.new,width=inner,height=caption_height,font_size=caption,color="onSurfaceVariant"},
            }
        end},list_height)
end
add(ui.Row{id="actions",width=content,height=button_height,gap=T.spacing.sm,
    kit.ActionButton{id="open",label=ui.tr("bead.panel.updates-open"),primary=true,
        on_activate=get("status.updates_can_open") and ui.action("updates.open") or nil,height=button_height},
    kit.ActionButton{id="refresh",label=ui.tr("bead.panel.updates-check"),
        on_activate=get("status.updates_can_refresh") and ui.action("updates.refresh") or nil,height=button_height},
},button_height)
return kit.Panel{id="updates-popout",edge="top",open=ui.bind("panel.open"),duration=ui.bind("view.duration"),
    fade=false,reveal=true,width=width,height=height,padding=padding,gap=gap,
    radius=T.radii.medium,fill="surface",table.unpack(kids)}
