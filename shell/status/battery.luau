-- SPDX-License-Identifier: GPL-3.0-only
-- Battery summary popout, sized to the bar's battery-width body.
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local level=ui.resolve_for_check(ui.bind("battery.level"))
local width=ui.resolve_for_check(ui.bind("view.width"))
local percent=ui.resolve_for_check(ui.bind("status.primary"))
local known=type(percent)=="string" and string.find(percent,"%%")~=nil
local padding=T.spacing.panel
local inner=width-padding*2
return kit.Panel{id="status-popout",edge="top",open=ui.bind("panel.open"),
    duration=ui.bind("view.duration"),width=width,height=ui.bind("view.height"),
    padding=padding,gap=T.spacing.sm,radius=T.radii.medium,fill="surface",
    ui.Text{id="percent",text=known and ui.bind("status.primary") or "—",width=inner,height=40,
        font_size=T.text.headline*4/3,text_align="center",color="onSurface"},
    ui.Box{id="track",width=inner,height=8,radius=4,fill="surfaceContainer",
        ui.Box{id="level",position="absolute",left=0,top=0,
            width=known and inner*math.max(0,math.min(100,level))/100 or 0,
            height=8,radius=4,fill="primary"}},
    ui.Text{id="estimate",text=known and ui.bind("status.secondary") or ui.bind("status.primary"),width=inner,height=20,
        font_size=T.text.caption*4/3,text_align="center",color="onSurfaceVariant"},
    ui.Row{id="profiles",width=inner,height=72,gap=T.spacing.sm,
        (function()
            local tiles={}
            for _,profile in ipairs({
                {id="saver",icon="eco",label="bead.osd.power-saver"},
                {id="balanced",icon="balance",label="bead.osd.balanced"},
                {id="performance",icon="bolt",label="bead.osd.performance"},
            }) do
                table.insert(tiles,ui.Box{id=profile.id,width=(inner-T.spacing.sm*2)/3,height=72,
                    radius=T.radii.large,fill="surfaceContainer",
                    ui.Column{width=(inner-T.spacing.sm*2)/3,height=72,align="center",justify="center",gap=T.spacing.xs,
                        ui.Text{text=profile.icon,family="Material Symbols Rounded",height=24,font_size=T.icons.medium*4/3,color="onSurfaceVariant"},
                        ui.Text{text=ui.tr(profile.label),height=24,font_size=T.text.caption*.9,text_align="center",color="onSurfaceVariant"}}
                })
            end
            return table.unpack(tiles)
        end)()},
    ui.Text{id="profiles-status",text=ui.bind("status.detail"),width=inner,height=20,
        font_size=T.text.caption*4/3,text_align="center",color="onSurfaceVariant"},
}
