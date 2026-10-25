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
    local size=desktop and math.min(96,(props.instance.slot or {}).w or 96) or t.brand_size
    local function image(id)
        local p={id=id,width=size,height=size}
        if style=="nacre" then p.path="bar/assets/nacre-logo.themed.svg"
        else p.icon=style=="custom" and (settings.custom_path or "") or style=="arch" and "distributor-logo-archlinux" or "cachyos" end
        return ui.Image(p)
    end
    local action=settings.action=="launcher" and ui.action("launcher.open")
        or ui.action("view.open",settings.action=="dashboard" and "dashboard-today" or settings.route)
    if desktop then
        return kit.Button{id="brand",width=size,height=size,padding=0,fill="surfaceContainer",radius=18,
            aria_label=ui.tr("bead.bar.open-launcher"),on_activate=action,image(nil)}
    end
    return launcher and kit.Button{id="brand",width=t.brand_size,height=t.brand_size,padding=0,fill="transparent",
        aria_label=ui.tr("bead.bar.open-launcher"),on_activate=action,image(nil)} or image("brand")
end
local function style(title,preferred,render_fn)
    return {title=title,min_span=1,max_span=60,preferred_span=preferred or 1,render=render_fn or render}
end
local function desktop_style(title)
    return {title=title,min_span=1,max_span=1200,preferred_span=96,render=render}
end
local icon_styles={
    nacre=style("Nacre"),arch=style("Arch"),cachyos=style("CachyOS"),custom=style("Custom icon"),
}
local desktop_styles={
    nacre=desktop_style("Nacre"),arch=desktop_style("Arch"),cachyos=desktop_style("CachyOS"),custom=desktop_style("Custom icon"),
}
return {id="launcher",title="Launcher",icon="widgets",category="bar",data={topics={},actions={}},
    settings={icon={type="choice",options={"nacre","arch","cachyos","custom"}},custom_path={type="text"},action={type="choice",options={"launcher","dashboard","custom"}},route={type="text"}},
    defaults={icon="nacre",custom_path="",action="launcher",route=""},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    forms={bar={default_style="nacre",styles=icon_styles},desktop={default_style="nacre",styles=desktop_styles}}
}
