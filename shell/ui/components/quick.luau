-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local M=require("./metrics")
local T=kit.tokens
local S=T.spacing
local pad=M.quick_padding
local width=M.quick_width-2*pad
local body=T.text.caption*4/3
local caption=T.text.caption*16/15
local function row(item)
    local opacity=item.enabled and 1 or M.disabled_opacity
    local ink=ui.alpha("onSurface",opacity)
    local muted=ui.alpha("onSurfaceVariant",opacity)
    local indicator_width=S.xxl+S.xs
    local indicator_height=T.radii.medium
    local thumb=S.compact
    local thumb_pad=(indicator_height-thumb)/2
    local label_left=S.huge+S.xs
    local state_width=S.huge+S.compact
    local reason_top=S.xxl+S.xs+S.hairline/2
    local reason=kit.Paragraph{id="reason",position="absolute",left=label_left,top=reason_top,
        width=width-label_left-S.md,text=item.reason,font_size=caption,advance=M.reason_advance,color=muted}
    local row_height=math.max(M.row_height,reason_top+reason.props.height+S.sm-S.hairline)
    return kit.Button{id=item.id,width=width,height=row_height,padding=0,radius=M.button_radius,
        activate_on_release=true,focus_on_pointer=false,
        fill=item.on and ui.tint("surfaceContainerHigh","primary",.12) or ui.alpha("surfaceContainerHigh",opacity),
        border_width=item.on and 1 or 0,border_color=ui.alpha("primary",.45*opacity),
        hover_fill=ui.tint("surfaceContainerHigh","onSurface",ui.resolve_for_check(ui.bind("bead.pressed")) and M.pressed_opacity or M.hover_opacity),
        aria_label=item.label..". "..item.status..". "..item.reason,
        on_activate=item.enabled and ui.resolve_for_check(ui.bind("panel.open")) and ui.action("launcher.quick",item.id) or nil,
        ui.Text{id="icon",position="absolute",left=S.compact+S.hairline/2,top=S.sm-S.hairline,width=S.xxl,height=S.xxl,
            text=item.on and "check" or M.quick_icons[item.id],family="Material Symbols Rounded",font_size=T.icons.small*4/3,color=ink},
        ui.Text{id="label",position="absolute",left=label_left,top=S.sm,width=width-label_left-S.huge*2-S.sm,height=S.xxl,
            text=item.label,font_size=body,color=ink},
        ui.Text{id="state",position="absolute",right=indicator_width+S.compact+S.sm-S.hairline,top=S.compact,width=state_width,height=S.xl,
            text=item.status,font_size=caption,text_align="right",color=muted},
        ui.Box{id="switch",position="absolute",right=S.compact,top=S.sm,width=indicator_width,height=indicator_height,
            radius=T.radii.small,fill=ui.alpha(item.on and "primary" or "surfaceContainerHighest",opacity),
            border_width=item.on and 0 or 1,border_color=ui.alpha("outline",opacity),
            ui.Box{id="thumb",position="absolute",left=item.on and indicator_width-thumb-thumb_pad or thumb_pad,top=thumb_pad,
                width=thumb,height=thumb,radius=thumb/2,fill=ui.alpha(item.on and "onPrimary" or "onSurfaceVariant",opacity)}},
        reason,
    }
end
return ui.component("LauncherQuick",{},function(_)
    local count=ui.resolve_for_check(ui.bind("launcher.updates.count"))
    local by_id,controls={},{}
    for _,item in ipairs(ui.resolve_for_check(ui.bind("launcher.quick"))) do by_id[item.id]=item end
    for _,id in ipairs(M.quick_order) do
        if by_id[id] then table.insert(controls,row(by_id[id])) end
    end
    return kit.Card{id="quick",position="absolute",right=S.xl,top=M.quick_top,width=M.quick_width,height=M.quick_height,
        padding=pad,gap=S.sm,radius=M.quick_radius,fill=ui.tint("surface","onSurface",.025),
        ui.Text{id="title",text=ui.tr("bead.launcher.quick-title"),font_size=body,height=S.xl,color="onSurface"},
        ui.Column{id="controls",gap=S.sm,table.unpack(controls)},
        count>=0 and ui.Row{id="updates",width=width,height=S.huge,gap=S.sm,padding_left=S.compact,align="center",
            radius=M.button_radius,fill="surfaceContainerHigh",
            ui.Text{id="icon",text="system_update",width=S.xl,height=S.xxl,family="Material Symbols Rounded",font_size=T.icons.small*4/3},
            ui.Text{id="count",text=count==0 and ui.tr("bead.dashboard.updates-current") or ui.tr("bead.dashboard.updates-count",{count=count}),
                width=width-S.compact-S.xl-S.sm,height=S.xl,font_size=body,color="onSurface"}}
            or ui.Box{id="updates",height=0,width=0},
        ui.Text{id="error",text=ui.bind("launcher.quick_error"),font_size=caption,height=S.huge,color="error"},
    }
end)
