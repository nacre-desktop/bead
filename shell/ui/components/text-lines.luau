-- SPDX-License-Identifier: GPL-3.0-only
-- Launcher copy: measured lines, explicit newlines and bounded overflow. Each Text
-- gets one measured line, so the renderer cannot wrap into an unreserved row.
local ui=require("bead.ui")
return function(p)
    local text=(p.text or ""):gsub("\r\n","\n"):gsub("\r","\n")
    local width=math.max(1,p.width*.98)
    local function fits(s) return ui.text_width(s,"IBM Plex Sans",p.font_size)<=width end
    local function prefix(s,n) return string.sub(s,1,(utf8.offset(s,n+1) or (#s+1))-1) end
    local height=math.max(p.line_height,math.ceil(ui.text_line_height("IBM Plex Sans",p.font_size)))
    local lines={}
    local rest=text
    for index=1,p.max_lines do
        local newline=string.find(rest,"\n",1,true)
        local part=newline and string.sub(rest,1,newline-1) or rest
        local truncated=index==p.max_lines and (newline~=nil or not fits(part))
        local suffix=truncated and "…" or ""
        local lo,hi=0,utf8.len(part) or #part
        while lo<hi do
            local mid=math.ceil((lo+hi)/2)
            if fits(prefix(part,mid)..suffix) then lo=mid else hi=mid-1 end
        end
        local line=prefix(part,lo)
        if lo<(utf8.len(part) or #part) and not truncated then
            local boundary=line:match("^.*()%s")
            if boundary and boundary>1 then line=string.sub(line,1,boundary-1) end
        end
        table.insert(lines,ui.Text{id="line-"..index,text=line..suffix,width=p.width,
            height=height,font_size=p.font_size,color=p.color,text_align=p.text_align,shrink=0})
        if truncated then break end
        if #line<#part then rest=string.sub(rest,#line+1):gsub("^ +","")
        elseif newline then rest=string.sub(rest,newline+1)
        else break end
    end
    return ui.Column{id=p.id,width=p.width,height=#lines*height,shrink=0,gap=0,table.unpack(lines)}
end
