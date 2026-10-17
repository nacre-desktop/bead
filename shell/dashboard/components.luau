-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local M={}
function M.rows(id,model,width,height,compact,empty)
    if #ui.resolve_for_check(ui.bind(model))==0 then
        return ui.Text{id=id,text=empty or "Data unavailable",width=width,height=40,font_size=14,color="onSurfaceVariant"}
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
return M
