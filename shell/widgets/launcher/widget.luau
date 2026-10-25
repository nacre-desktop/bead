-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local kit=c.kit
local t=c.t
local launcher=c.launcher
return {id="launcher",title="Launcher",icon="widgets",category="bar",data={topics={},actions={}},settings={icon={type="choice",options={"nacre","arch","cachyos","custom"}},custom_path={type="text"},action={type="choice",options={"launcher","dashboard","custom"}},route={type="text"}},defaults={icon="nacre",custom_path="",action="launcher",route=""},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},
    variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
            local function image(id)
                local props={id=id,width=t.brand_size,height=t.brand_size}
                if settings.icon=="nacre" then props.path="bar/assets/nacre-logo.themed.svg"
                else props.icon=settings.icon=="custom" and settings.custom_path or settings.icon=="arch" and "distributor-logo-archlinux" or "cachyos" end
                return ui.Image(props)
            end
            local action=settings.action=="launcher" and ui.action("launcher.open") or ui.action("view.open",settings.action=="dashboard" and "dashboard-today" or settings.route)
            return launcher and kit.Button{id="brand",width=t.brand_size,height=t.brand_size,padding=0,fill="transparent",aria_label=ui.tr("bead.bar.open-launcher"),on_activate=action,image(nil)} or image("brand")
    end}}}
