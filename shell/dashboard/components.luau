-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local M={}
function M.rows(id,model,width,height,compact,empty)
    if #ui.resolve_for_check(ui.bind(model))==0 then
        return ui.Text{id=id,text=empty or ui.tr("bead.dashboard.data-unavailable"),width=width,height=40,font_size=14,color="onSurfaceVariant"}
    end
    return ui.Column{id=id,width=width,height=height,gap=compact and 4 or 8,
        ui.Repeat{id="items",model=ui.bind(model),key="id",render=function(item)
            return ui.Row{id="row",width=width,height=compact and 24 or 40,gap=8,align="center",
                ui.Text{id="label",text=item.label,width=width*.48,height=24,font_size=compact and 14 or 16,color="onSurface",shrink=0},
                ui.Column{width=width*.48,height=compact and 24 or 40,gap=0,
                    ui.Text{id="value",text=item.value,height=22,font_size=compact and 14 or 16,text_align="right",color="primary"},
                    not compact and ui.Text{id="detail",text=item.detail,height=18,font_size=12,text_align="right",color="onSurfaceVariant"} or nil,
                },
            }
        end},
    }
end
function M.title(id,text,width)
    return ui.Text{id=id,text=text,width=width,height=28,font_size=20,color="onSurface"}
end
-- One-line rows that fill the width; `act` rows activate a daemon call when enabled.
function M.lines(id,model,width,height,empty,act,enabled)
    if #ui.resolve_for_check(ui.bind(model))==0 then
        return ui.Text{id=id,text=empty,width=width,height=40,font_size=14,color="onSurfaceVariant"}
    end
    return ui.Column{id=id,width=width,height=height,gap=4,
        ui.Repeat{id="items",model=ui.bind(model),key="id",render=function(item)
            local body=ui.Row{id="row",width=width-16,height=32,gap=8,align="center",
                ui.Text{id="label",text=item.label,width=width-110,height=24,font_size=16,color="onSurface",shrink=0},
                ui.Text{id="value",text=item.value,width=70,height=22,font_size=14,text_align="right",color="primary"},
            }
            if not act then return body end
            return kit.Button{id="press",width=width,height=40,padding=8,radius=10,fill="transparent",aria_label=item.label,
                on_activate=enabled and ui.action("dashboard.do",item.act) or nil,body}
        end},
    }
end
function M.button(id,label,action,width,selected,aria)
    width=width or 160
    return kit.Button{id=id,width=width,height=40,padding=0,radius=12,justify="center",align="center",
        fill=selected and ui.alpha("primary",0.12) or "surfaceContainerLow",aria_label=aria or label,on_activate=action,
        ui.Text{id=id.."-label",text=label,width=width-16,height=24,font_size=16,text_align="center",
            color=action and "primary" or "onSurfaceVariant"}}
end
return M
