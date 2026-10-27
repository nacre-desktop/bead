-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local launcher=c.launcher
local function render(props,settings,_data)
    settings=settings or {}
    local style=(props.instance and props.instance.style) or settings.icon or props.form_style or "nacre"
    local desktop=props.instance and props.instance.host=="canvas"
    local size=desktop and math.min(style=="cachyos" and 78 or style=="arch" and 108 or 92,(props.instance.slot or {}).w or 92) or t.brand_size
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
        for _,app in ui.resolve_for_check(ui.bind("launcher.apps")) or {} do
            if app.favorite==true and #favorites<5 then
                table.insert(favorites,kit.Button{id="favorite-"..app.id,width=size,height=size,padding=kit.tokens.spacing.xs,
                    fill="surfaceContainerHigh",hover_fill="surfaceContainerHighest",radius=kit.tokens.radii.medium,
                    aria_label=app.name,on_activate=ui.action("launcher.open"),
                    ui.Column{id="favorite",width="fill",height="fill",align="center",justify="center",gap=kit.tokens.spacing.xs,
                        ui.Image{id="icon",icon=app.icon,width=36,height=36,aria_label=app.name},
                        ui.Text{id="name",text=app.name,width="fill",height=20,font_size=kit.tokens.text.caption*4/3,text_align="center",color="onSurface"}}})
            end
        end
        if #favorites==0 then table.insert(favorites,kit.Button{id="open-launcher",width=size,height=size,padding=0,
            fill="surfaceContainerHigh",radius=kit.tokens.radii.medium,aria_label=ui.tr("bead.bar.open-launcher"),on_activate=action,image(nil)}) end
        return ui.Row{id="favorites-dock",width=(size+kit.tokens.spacing.xs)*#favorites,height=size+kit.tokens.spacing.xl,
            padding=kit.tokens.spacing.xs,gap=kit.tokens.spacing.xs,radius=kit.tokens.radii.large,
            fill=style=="custom" and "surfaceContainerHigh" or style=="arch" and "surfaceContainer" or "surfaceContainerHigh",table.unpack(favorites)}
    end
    return launcher and kit.Button{id="brand",width=t.brand_size,height=t.brand_size,padding=0,fill="transparent",
        aria_label=ui.tr("bead.bar.open-launcher"),on_activate=action,image(nil)} or image("brand")
end
local function style(title,preferred,render_fn)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred or 1,render=render_fn or render}
end
local function desktop_style(title,span)
    return {title=title,min_span=1,max_span=1200,preferred_span=span or 440,preferred_height=112,render=render}
end
local icon_styles={
    nacre=style("Nacre"),arch=style("Arch"),cachyos=style("CachyOS"),custom=style("Custom icon"),
}
local desktop_styles={
    nacre=desktop_style("Favorites dock"),arch=desktop_style("Wide dock",520),cachyos=desktop_style("Compact dock",360),custom=desktop_style("Custom dock",440),
}
return {id="launcher",title="Launcher",icon="apps",category="bar",data={topics={},actions={}},
    settings={icon={type="choice",hidden=true,options={"nacre","arch","cachyos","custom"}},custom_path={type="text",advanced=true},action={type="choice",options={"launcher","dashboard","custom"}},route={type="text",advanced=true}},
    defaults={icon="nacre",custom_path="",action="launcher",route=""},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={legacy_style_setting="icon",default_style="nacre",styles=icon_styles},desktop={legacy_style_setting="icon",default_style="nacre",styles=desktop_styles}}
}
