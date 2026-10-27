-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local launcher=c.launcher
local visual=require("../desktop-visuals")
local function render(props,settings,_data)
    settings=settings or {}
    local style=(props.instance and props.instance.style) or settings.icon or props.form_style or "nacre"
    local desktop=props.instance and props.instance.host=="canvas"
    local size=desktop and 52 or t.brand_size
    local function image(id)
        local p={id=id,width=size,height=size}
        if style=="nacre" then p.path="bar/assets/nacre-logo.themed.svg"
        else p.icon=style=="custom" and (settings.custom_path or "") or style=="arch" and "distributor-logo-archlinux" or "cachyos" end
        return ui.Image(p)
    end
    local action=settings.action=="launcher" and ui.action("launcher.open")
        or ui.action("view.open",settings.action=="dashboard" and "dashboard-today" or settings.route)
    if desktop then
        local favorites={}
        local hovered=ui.resolve_for_check(ui.bind("bead.hovered")) or ""
        for _,app in ui.resolve_for_check(ui.bind("launcher.apps")) or {} do
            if app.favorite==true and #favorites<5 then
                local over=string.find(hovered,"favorite-"..tostring(app.id),1,true)~=nil
                local icon_size=over and 64 or size
                favorites[#favorites+1]=kit.Button{id="favorite-"..app.id,width=icon_size,height=icon_size,padding=0,
                    fill=ui.alpha("onSurface",.08),hover_fill=ui.alpha("onSurface",.12),radius=over and 18 or 15,
                    aria_label=app.name,on_activate=ui.action("menu.activate",app.id),
                    ui.Image{id="icon",icon=app.icon,width=icon_size*.68,height=icon_size*.68,aria_label=app.name}}
            end
        end
        if #favorites==0 then table.insert(favorites,kit.Button{id="open-launcher",width=size,height=size,padding=0,
            fill=ui.alpha("onSurface",.08),radius=15,aria_label=ui.tr("bead.bar.open-launcher"),on_activate=action,image(nil)}) end
        local gap=kit.tokens.spacing.sm
        local dock_width=#favorites*(size+gap)+gap+36
        table.insert(favorites,kit.Button{id="add-favorite",width=32,height=32,padding=0,radius=16,
            fill=ui.alpha("onSurface",.08),hover_fill=ui.alpha("onSurface",.14),aria_label="Open app launcher",
            on_activate=ui.action("view.open","launcher"),
            ui.Text{id="add-icon",text="add",family="Material Symbols Rounded",font_size=kit.tokens.icons.small,width=32,height=32,color=visual.secondary}})
        return visual.glass("favorites-dock",dock_width,92,kit.tokens.spacing.sm,{
            ui.Row{id="favorites",width="fill",height=68,gap=gap,align="end",table.unpack(favorites)}
        })
    end
    return launcher and kit.Button{id="brand",width=t.brand_size,height=t.brand_size,padding=0,fill="transparent",
        aria_label=ui.tr("bead.bar.open-launcher"),on_activate=action,image(nil)} or image("brand")
end
local function style(title,preferred,render_fn)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred or 1,render=render_fn or render}
end
local icon_styles={
    nacre=style("Nacre"),arch=style("Arch"),cachyos=style("CachyOS"),custom=style("Custom icon"),
}
local desktop_styles={nacre={title="Favorites dock",min_span=1,max_span=1200,preferred_span=440,preferred_height=92,render=render},
    arch={title="Legacy wide dock",min_span=1,max_span=1200,preferred_span=520,preferred_height=112,hidden=true,render=render},
    cachyos={title="Legacy compact dock",min_span=1,max_span=1200,preferred_span=360,preferred_height=92,hidden=true,render=render},
    custom={title="Legacy custom dock",min_span=1,max_span=1200,preferred_span=440,preferred_height=92,hidden=true,render=render}}
return {id="launcher",title="Launcher",icon="apps",category="bar",data={topics={},actions={}},
    settings={icon={type="choice",hidden=true,options={"nacre","arch","cachyos","custom"}},custom_path={type="text",advanced=true},action={type="choice",options={"launcher","dashboard","custom"}},route={type="text",advanced=true}},
    defaults={icon="nacre",custom_path="",action="launcher",route=""},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={legacy_style_setting="icon",default_style="nacre",styles=icon_styles},desktop={legacy_style_setting="icon",default_style="nacre",styles=desktop_styles}}
}
