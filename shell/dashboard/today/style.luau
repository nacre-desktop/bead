-- SPDX-License-Identifier: GPL-3.0-only
-- Today overview geometry follows the dashboard canvas rather than shell settings.
local M={}
function M.layout(width,height)
    local summary=math.max(288,math.min(420,height*.38))
    local gap=18
    local hero=height-summary-gap
    local calendar=width*.31
    local spacing=16
    local host=width*.38-spacing
    local media=width-calendar-host-spacing*2
    local art=math.max(40,math.min(media-48,math.min(200,math.max(132,media*.36)),summary-180))
    return {width=width,height=height,hero=hero,summary=summary,gap=gap,calendar=calendar,host=host,
        media=media,hostBody=184,metrics=92,hostGap=12,summaryGap=spacing,
        heroInset=24,heroTimeSize=math.max(44,math.min(64,44*hero/254))*4/3,heroDateSize=13*4/3,heroWeatherSize=11*4/3,
        heroTimeHeight=72,heroLineHeight=20,heroTextGap=10,heroRadius=20,cardRadius=18,mediaPadding=12,
        heroTextColor={255,253,250,255},
        artSize=art,
        artContainer=art+56,
        logoSize=math.min(156,math.max(124,host*.28)),logoContent=.74,hostRowHeight=24,hostRowGap=4,
        hostSmallRowHeight=27,hostSmallRowGap=6,
        hostTextSize=12*4/3,hostIconSize=13*4/3,
        mediaTitleSize=12*4/3,mediaArtistSize=10*4/3,mediaArtIconSize=36*4/3,
        mediaMetadataTitleHeight=20,mediaMetadataArtistHeight=18,mediaMetadataGap=4,mediaMetadataTail=16,
        mediaControlsY=math.min(summary-48,8+(art+56)+8+20+4+18+16),
        hostInfo=host-math.min(156,math.max(124,host*.28))-48}
end
return M
