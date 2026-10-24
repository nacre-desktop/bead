-- SPDX-License-Identifier: GPL-3.0-only
-- Nacre adapter: WMO only selects local illustration; photos are selected by city.
local ui=require("bead.ui")
local style=require("./weather-style")
local M={}
function M.scene(code)
    if code<0 then return "unknown" end
    if code<=1 then return "clear" end
    if code<=3 then return "clouds" end
    if code==45 or code==48 then return "mist" end
    if code>=71 and code<=77 or code==85 or code==86 then return "snow" end
    if code>=95 then return "storm" end
    return "rain"
end
function M.background()
    local code=ui.resolve_for_check(ui.bind("dashboard.weather.code"))
    local day=ui.resolve_for_check(ui.bind("dashboard.weather.day"))
    local city=ui.resolve_for_check(ui.bind("dashboard.heading"))
    local photo=style.photos.enabled and (style.photos.city[city] or style.photos.fallback)
    -- No remote URLs, fetches or ambient cache lookup. Attribution travels with a local asset.
    if photo then
        assert(type(photo.path)=="string" and not string.find(photo.path,":",1,true),"weather photo needs a bundle-relative path")
        assert(photo.width>0 and photo.height>0 and photo.author~="" and photo.license~="","weather photo needs dimensions and credit")
        local scale=math.max(style.width/photo.width,style.height/photo.height)
        local w,h=photo.width*scale,photo.height*scale
        return ui.Box{id="sky",position="absolute",left=0,top=0,width=style.width,height=style.height,
            ui.Scroll{id="crop",width=style.width,height=style.height,scroll=ui.bind("dashboard.weather.scroll"),
                ui.Image{id="photo",position="absolute",left=(style.width-w)/2,top=(style.height-h)/2,width=w,height=h,path=photo.path}},
            ui.Image{id="shade",position="absolute",left=0,top=0,width=style.width,height=style.height,path=style.photo_overlay},
            ui.Text{id="credit",position="absolute",left=style.padding,top=style.height-24,width=style.width-2*style.padding,height=18,font_size=style.detail_size,color=style.muted,text=photo.author.." · "..photo.license},
        }
    end
    local scene=M.scene(code)
    if code==2 then scene="partly" end
    return ui.Box{id="sky",position="absolute",left=0,top=0,width=style.width,height=style.height,
        ui.Image{id="illustration",width=style.width,height=style.height,path=style.sky..scene..(day and "-day" or "-night")..".themed.svg"},
        ui.Image{id="shade",position="absolute",left=0,top=0,width=style.width,height=style.height,path=style.overlay},
    }
end
return M
