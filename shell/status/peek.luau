-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local T=kit.tokens
local function get(name) return ui.resolve_for_check(ui.bind(name)) end
local devices=get("status.peek.devices") or {}
local summary=get("status.peek.summary") or get("status.primary") or ""
local detail=get("status.peek.detail") or ""
if get("status.kind")=="battery" then
    for _,source in ipairs(get("cc.src") or {}) do
        if source.id=="power.profile" and source.available and source.status~="" then
            detail=detail~="" and detail.." · "..source.status or source.status
        end
    end
end
local title=get("status.peek.title") or get("status.heading") or ""
local row_limit=4
local rows=math.min(#devices,row_limit)
local more_count=#devices-rows
local more_text=more_count>0 and ui.tr("bead.panel.bluetooth-peek-more",{count=more_count}) or ""
local function lines(text,per_line)
    return math.min(3,math.max(1,math.ceil((utf8.len(text) or #text)/per_line)))
end
local title_height=18
local summary_height=20*lines(summary,36)
local detail_height=detail~="" and 16*lines(detail,42) or 0
local more_height=more_text~="" and 16 or 0
local child_count=2+(detail_height>0 and 1 or 0)+(rows>0 and 1 or 0)+(more_height>0 and 1 or 0)
local body_height=title_height+summary_height+detail_height+rows*16+more_height
    +6*(child_count-1)+(rows>0 and 6*(rows-1) or 0)
-- Build a dense child sequence: nil optional entries make table.unpack truncate later rows.
local content={
    ui.Text{id="heading",text=title,width=280,height=title_height,font_size=T.text.caption*4/3,font_weight=500,color="onSurfaceVariant"},
    ui.Text{id="summary",text=ui.bind("status.peek.summary"),width=280,height=summary_height,font_size=T.text.body*4/3,color="onSurface"},
}
if detail~="" then
    table.insert(content,ui.Text{id="detail",text=detail,width=280,height=detail_height,font_size=T.text.caption*4/3,color="onSurfaceVariant"})
end
if rows>0 then
    local device_rows={}
    for i=1,rows do
        local item=devices[i]
        local row_content={ui.Text{id="device-name-"..i,text=item.name,width=220,height=16,font_size=T.text.caption*4/3,color="onSurface"}}
        if item.battery~="" then
            table.insert(row_content,ui.Row{id="device-battery-"..i,width=54,height=16,gap=4,align="center",
                ui.Text{id="battery-icon-"..i,text="battery_full",family="Material Symbols Rounded",width=16,height=16,font_size=16,color="primary"},
                ui.Text{id="battery-level-"..i,text=item.battery,width=34,height=16,font_size=T.text.caption*1.2,color="onSurface"},
            })
        end
        table.insert(device_rows,ui.Row{id="device-"..i,width=280,height=16,gap=6,align="center",table.unpack(row_content)})
    end
    table.insert(content,ui.Column{id="devices",width=280,height=rows*16+6*(rows-1),gap=6,
        table.unpack(device_rows)
    })
end
if more_height>0 then
    table.insert(content,ui.Text{id="more-devices",text=more_text,width=280,height=16,font_size=T.text.caption*4/3,color="onSurfaceVariant"})
end
return kit.Panel{id="status-peek",edge="top",open=ui.bind("panel.open"),duration=0.23,transition="ease-in-out",fade=false,reveal=true,
    width=310,height=body_height+30,padding=15,gap=6,
    radius=kit.tokens.radii.medium,fill="surfaceContainerHigh",table.unpack(content),
}
