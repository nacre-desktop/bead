-- SPDX-License-Identifier: GPL-3.0-only
-- Nacre adapter: WMO only selects local illustration; photos are selected by city.
local ui=require("bead.ui")
local kit=require("bead.kit")
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
-- The photo to show, if any: the daemon's leased city photo (`dashboard.weather.photo*`, with its
-- Commons credit link) first, then the optional local map below. Attribution travels with both.
local function photo(city)
    local function get(name) return ui.resolve_for_check(ui.bind("dashboard.weather."..name)) end
    if get("photo") then
        return {width=get("photo_w"),height=get("photo_h"),author=get("photo_author"),license=get("photo_license"),
            image=function(w,h) return ui.Image{id="photo",icon="weather-photo",width=w,height=h,shrink=0} end,link=get("photo_link")}
    end
    local local_photo=get("photos_enabled") and style.photos.enabled and (style.photos.city[city] or style.photos.fallback)
    if not local_photo then return nil end
    -- No remote URLs, fetches or ambient cache lookup here. Attribution travels with a local asset.
    assert(type(local_photo.path)=="string" and not string.find(local_photo.path,":",1,true),"weather photo needs a bundle-relative path")
    assert(local_photo.width>0 and local_photo.height>0 and local_photo.author~="" and local_photo.license~="","weather photo needs dimensions and credit")
    return {width=local_photo.width,height=local_photo.height,author=local_photo.author,license=local_photo.license,
        image=function(w,h) return ui.Image{id="photo",width=w,height=h,shrink=0,path=local_photo.path} end}
end
function M.background()
    local code=ui.resolve_for_check(ui.bind("dashboard.weather.code"))
    local day=ui.resolve_for_check(ui.bind("dashboard.weather.day"))
    local found=photo(ui.resolve_for_check(ui.bind("dashboard.weather.city")))
    if found then
        local scale=math.max(style.width/found.width,style.height/found.height)
        local credit=ui.tr("bead.dashboard.weather-photo-credit",{author=found.author,license=found.license})
        local w=style.width-2*style.padding
        return ui.Box{id="sky",position="absolute",left=0,top=0,width=style.width,height=style.height,
            ui.Scroll{id="crop",width=style.width,height=style.height,scroll=ui.bind("dashboard.weather.scroll"),
                ui.Row{id="centre",width=style.width,height=style.height,shrink=0,justify="center",align="center",
                    found.image(found.width*scale,found.height*scale)}},
            ui.Image{id="shade",position="absolute",left=0,top=0,width=style.width,height=style.height,path=style.photo_overlay},
            found.link and kit.Button{id="credit-link",position="absolute",left=style.padding,top=style.height-24,width=w,height=18,padding=0,
                fill="transparent",hover_fill="transparent",aria_label=credit,on_activate=ui.action("dashboard.do","weather.credit"),
                ui.Text{id="credit",width=w,height=18,font_size=style.detail_size,color=style.muted,text=credit}}
            or ui.Text{id="credit",position="absolute",left=style.padding,top=style.height-24,width=w,height=18,font_size=style.detail_size,color=style.muted,text=credit},
        }
    end
    local scene=M.scene(code)
    if code==2 then scene="partly" end
    local shade = style.overlay
        and ui.Image{id="shade",position="absolute",left=0,top=0,width=style.width,height=style.height,path=style.overlay}
        or ui.Group{id="shade",width=0,height=0}
    return ui.Box{id="sky",position="absolute",left=0,top=0,width=style.width,height=style.height,
        ui.Image{id="illustration",width=style.width,height=style.height,path=style.sky..scene..(day and "-day" or "-night")..".themed.svg"},
        shade,
    }
end
return M
