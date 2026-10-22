-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local ready=ui.resolve_for_check(ui.bind("status.ready"))
local active=ui.resolve_for_check(ui.bind("status.active"))
local enabled=ui.resolve_for_check(ui.bind("network.wifi_enabled"))
local padding=kit.tokens.spacing.md
local icon_size=kit.tokens.icons.xlarge*4/3
local width=ui.resolve_for_check(ui.bind("view.width"))
local duration=ui.resolve_for_check(ui.bind("view.duration"))
local height=math.max(ui.resolve_for_check(ui.bind("network.height")),ui.resolve_for_check(ui.bind("view.height")))
local text_width=width-padding*2-icon_size-kit.tokens.spacing.md
return kit.Panel{id="status-popout",edge=ui.resolve_for_check(ui.bind("view.edge")),
    open=ui.bind("panel.open"),duration=duration,width=width,height=height,padding=padding,
    gap=kit.tokens.spacing.sm,radius=kit.tokens.radii.medium,fill="surface",
    ui.Text{id="heading",text=ui.bind("status.heading"),height=24,font_size=kit.tokens.text.title*4/3,color="onSurface"},
    ui.Row{id="summary",height=88,gap=kit.tokens.spacing.md,align="center",
        ui.Text{id="icon",text=ui.bind("status.icon"),family="Material Symbols Rounded",font_size=icon_size,
            width=icon_size,height=42,shrink=0,color=ready and active and "primary" or "onSurfaceVariant"},
        ui.Column{width=text_width,height=84,gap=2,
            ui.Text{id="primary",text=ui.bind("status.primary"),font_size=kit.tokens.text.body*4/3,height=60,color="onSurface"},
            ui.Text{id="secondary",text=ui.bind("status.secondary"),font_size=kit.tokens.text.caption*4/3,height=22,color="onSurfaceVariant"},
        },
    },
    ui.Text{id="detail",text=ui.bind("status.detail"),font_size=kit.tokens.text.caption*4/3,height=24,color="onSurfaceVariant"},
    ui.Row{id="wifi-row",height=36,align="center",gap=kit.tokens.spacing.md,
        ui.Text{id="wifi-label",text=ui.tr("bead.panel.wifi"),height=28,grow=1,font_size=kit.tokens.text.body*4/3,
            color=enabled and "onSurface" or "onSurfaceVariant"},
        kit.Switch{id="wifi-radio",checked=ui.bind("network.wifi_checked"),aria_label=ui.tr("bead.panel.wifi-radio"),
            on_activate=enabled and ui.action("network.toggle_wifi") or nil},
    },
    ui.Text{id="wifi-message",text=ui.bind("network.wifi_message"),font_size=kit.tokens.text.caption*4/3,height=20,color="onSurfaceVariant"},
    ui.Repeat{id="links",model=ui.bind("network.rows"),key="id",render=function(item)
        return kit.Button{id="link",height=32,padding=kit.tokens.spacing.sm,fill="transparent",
            aria_label=item.label,
            on_activate=item.enabled and (item.active and ui.action("network.disconnect",item.id) or ui.action("network.connect",item.id)) or nil,
            ui.Row{id="link-row",height=28,align="center",gap=kit.tokens.spacing.md,
                ui.Text{id="link-name",text=item.label,height=24,grow=1,font_size=kit.tokens.text.body*4/3,color=item.enabled and "onSurface" or "onSurfaceVariant"},
                ui.Text{id="link-action",text=item.action,height=24,font_size=kit.tokens.text.caption*4/3,color=item.active and "primary" or "onSurfaceVariant"},
            },
        }
    end},
    ui.Text{id="link-message",visible=ui.bind("network.link_visible"),text=ui.bind("network.link_message"),font_size=kit.tokens.text.caption*4/3,height=20,color="onSurfaceVariant"},
}
