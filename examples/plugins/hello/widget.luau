-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Plugin API 1: the entry returns a factory. `api.ui`/`api.kit` are the Bead UI and kit,
-- `api.plugin` holds the plugin id and its settings. Sizes and colours come from kit tokens.
return function(api)
    local ui,kit=api.ui,api.kit
    local sp,tx=kit.tokens.spacing,kit.tokens.text
    local size=tx.body*4/3
    local function bar(props,settings)
        local greeting=tostring(settings.greeting or "Hello")
        local width=math.ceil(ui.text_width(greeting,"IBM Plex Sans",size))
        return ui.Row{id="hello",height=kit.tokens.icons.large*4/3+sp.sm,shrink=0,align="center",gap=sp.xs,
            padding_left=sp.sm,padding_right=sp.sm,
            settings.show_icon~=false and ui.Text{id="hello-icon",text="waving_hand",family="Material Symbols Rounded",
                font_size=kit.tokens.icons.large*4/3,width=24,height=28,color="secondary"} or nil,
            ui.Text{id="hello-text",text=greeting,width=width,height=20,font_size=size,color="secondary"}}
    end
    local function style(title,preferred)
        return {title=title,min_span=1,max_span=60,preferred_span=preferred,render=bar}
    end
    return {id="hello",icon="waving_hand",category="bar",data={topics={},actions={}},
        settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock"}},
        forms={bar={default_style="text",styles={text=style("Greeting",3)}}}}
end
