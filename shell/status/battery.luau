-- SPDX-License-Identifier: GPL-3.0-only
-- Battery summary popout, sized to the bar's battery-width body.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local level=ui.resolve_for_check(ui.bind("battery.level"))
local width=ui.resolve_for_check(ui.bind("view.width"))
local percent=ui.resolve_for_check(ui.bind("status.primary"))
local known=type(percent)=="string" and string.find(percent,"%%")~=nil
local choices={}
local active=""
for _,source in ipairs(ui.resolve_for_check(ui.bind("cc.src")) or {}) do
    if source.id=="power.profile" then choices=source.choices or {};active=source.status end
end
local padding=T.spacing.panel
local inner=width-padding*2
return kit.Panel{id="status-popout",edge="top",open=ui.bind("panel.open"),
    duration=ui.bind("view.duration"),width=width,height=ui.bind("view.height"),
    padding=padding,gap=T.spacing.sm,radius=T.radii.medium,fill="surface",
    ui.Text{id="percent",text=known and ui.bind("status.primary") or "—",width=inner,height=40,
        family="JetBrainsMono Nerd Font",font_size=T.text.headline*4/3,text_align="center",color="onSurface"},
    ui.Box{id="track",width=inner,height=8,radius=4,fill="surfaceContainer",
        ui.Box{id="level",position="absolute",left=0,top=0,
            width=known and inner*math.max(0,math.min(100,level))/100 or 0,
            height=8,radius=4,fill="primary"}},
    ui.Text{id="estimate",text=known and ui.bind("status.secondary") or ui.bind("status.primary"),width=inner,height=20,
        font_size=(T.text.caption+1)*4/3,text_align="center",color="onSurfaceVariant"},
    ui.Text{id="profile-name",text=active~="" and active or ui.bind("status.detail"),width=inner,height=20,
        font_size=(T.text.caption+1)*4/3,text_align="center",color="onSurface"},
    ui.Row{id="profiles",width=inner,height=72,gap=T.spacing.sm,
        (function()
            local tiles={}
            for _,profile in ipairs({
                {id="power-saver",icon="eco",label="bead.osd.power-saver"},
                {id="balanced",icon="balance",label="bead.osd.balanced"},
                {id="performance",icon="bolt",label="bead.osd.performance"},
            }) do
                local choice=nil
                for _,item in ipairs(choices) do
                    if string.lower(item.name)==profile.id or item.id==profile.id then choice=item end
                end
                local selected=choice and choice.active
                local label=ui.tr(profile.label)
                local tile_width=(inner-T.spacing.sm*2)/3
                local label_size=(T.text.caption-1)*4/3
                local label_height=math.min(2,math.ceil((utf8.len(label) or #label)*label_size*.5/tile_width))*T.spacing.xl
                local ink=selected and "onPrimaryContainer" or "onSurfaceVariant"
                table.insert(tiles,kit.Button{id=profile.id,padding=0,
                    on_activate=choice and ui.action("cc.do","choose:power.profile:"..choice.id) or nil,width=(inner-T.spacing.sm*2)/3,height=72,
                    radius=T.radii.large,fill=selected and "primaryContainer" or "surfaceContainer",
                    ui.Column{width=(inner-T.spacing.sm*2)/3,height=72,align="center",justify="center",gap=T.spacing.xs,
                        ui.Text{text=profile.icon,family="Material Symbols Rounded",height=24,font_size=T.icons.medium*4/3,color=ink},
                        ui.Text{text=label,width=tile_width,height=label_height,font_size=label_size,text_align="center",color=ink}}
                })
            end
            return table.unpack(tiles)
        end)()},
    ui.Text{id="profiles-status",text=ui.bind("cc.message"),width=inner,height=20,
        font_size=T.text.caption*4/3,text_align="center",color="onSurfaceVariant"},
}
