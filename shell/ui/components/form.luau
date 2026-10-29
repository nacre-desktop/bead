-- SPDX-License-Identifier: GPL-3.0-only
-- The launcher's inline forms (B0199): set a reminder, create a TUI or web app, import a theme or an
-- extension, choose the recording sound, and the confirmation for restart / install mise. QML drew these
-- as `NacreMenuForm` over the whole launcher; geometry follows it (a 520 px column, 16 px apart, the
-- buttons fixed at the foot). Field values live in `launcher.form.v1..v4`; the Rust side
-- (`launcher_form.rs`) fills the starting values, validates and runs the backend.
local ui=require("bead.ui")
local kit=require("bead.kit")
local G={column=520,top=24,gap=16,title_font=24,label_font=14.667,text_font=14.667,input_height=44,multiline_height=110,
    input_radius=8,input_padding=12,input_font=16,choice_height=40,choice_radius=9,button_height=38,button_radius=9,
    button_gap=10,button_pad=12,button_font=14.667,char_width=7.6,footer_bottom=24,error_height=20}
-- One entry per form kind. `title`/`description` are i18n ids (a missing title is the row's own text in
-- `launcher.form.message`); a field is a text slot, or a choice with `choices`.
local FORMS={
    reminder={title="bead.launcher.form.reminder",button="bead.launcher.form.reminder-button",fields={
        {slot=1,label="bead.launcher.form.minutes"},
        {slot=2,label="bead.launcher.form.message",placeholder="bead.launcher.form.message-placeholder"}}},
    webapp={title="bead.launcher.form.webapp",description="bead.launcher.form.webapp-description",button="bead.launcher.form.create-app",fields={
        {slot=1,label="bead.launcher.form.app-name",placeholder="bead.launcher.form.app-name-placeholder"},
        {slot=2,label="bead.launcher.form.website",placeholder_text="https://…"}}},
    tui={title="bead.launcher.form.tui",description="bead.launcher.form.tui-description",button="bead.launcher.form.create-app",fields={
        {slot=1,label="bead.launcher.form.app-name"},
        {slot=2,label="bead.launcher.form.tui-command",multiline=true},
        {slot=3,label="bead.launcher.form.tui-icon"},
        {slot=4,label="bead.launcher.form.tui-terminal",choices={
            {value="kitty",label="Kitty"},{value="foot",label="Foot"},{value="alacritty",label="Alacritty"},{value="wezterm",label="WezTerm"}}}}},
    ["theme-import"]={title="bead.launcher.form.theme-import",button="bead.launcher.form.theme-import-button",fields={
        {slot=1,label="bead.launcher.form.theme-file",placeholder_text="~/Downloads/nacre.extension.json"}}},
    ["extension-import"]={title="bead.launcher.form.extension-import",description="bead.launcher.form.extension-description",button="bead.launcher.form.extension-button",fields={
        {slot=1,label="bead.launcher.form.repository",placeholder_text="https://github.com/owner/extension"}}},
    ["recording-area"]={description="bead.launcher.form.recording-description",button="bead.launcher.form.recording-start",fields={
        {slot=1,label="bead.launcher.form.recording-sound",choices={
            {value="none",label_id="bead.launcher.form.recording-none"},
            {value="system",label_id="bead.launcher.form.recording-system"},
            {value="mixed",label_id="bead.launcher.form.recording-mixed"}}}}},
    ["confirm-restart"]={description="bead.launcher.form.restart-description",button="bead.launcher.form.continue",fields={}},
    ["confirm-mise"]={description="bead.launcher.form.mise-description",button="bead.launcher.form.continue",fields={}},
}
FORMS["recording-screen"]=FORMS["recording-area"]
local function get(name) return ui.resolve_for_check(ui.bind(name)) end

local function field(def)
    local children={ui.Text{id="label",width="fill",height=20,text=ui.tr(def.label),font_size=G.label_font,color="onSurface"}}
    local id="f"..def.slot
    if def.choices then
        local chosen=get("launcher.form.v"..def.slot)
        for _,choice in ipairs(def.choices) do
            local label=choice.label_id and ui.tr(choice.label_id) or choice.label
            local on=chosen==choice.value
            children[#children+1]=kit.Button{id="choice-"..choice.value,width="fill",height=G.choice_height,padding_left=G.input_padding,
                radius=G.choice_radius,activate_on_release=true,focus_on_pointer=false,aria_label=label,
                fill=on and ui.tint("surfaceContainerHigh","primary",.16) or "surfaceContainerHigh",
                border_width=on and 1 or 0,border_color=ui.alpha("primary",.6),
                hover_fill=ui.tint("surfaceContainerHigh","onSurface",.08),
                on_activate=ui.action("launcher.form.choose",def.slot..":"..choice.value),
                ui.Text{text=label,width="fill",height=20,font_size=G.text_font,color="onSurface"}}
        end
    else
        children[#children+1]=kit.TextInput{id=id,width="fill",height=def.multiline and G.multiline_height or G.input_height,
            multiline=def.multiline or nil,text=ui.bind("launcher.form.v"..def.slot),
            placeholder=def.placeholder and ui.tr(def.placeholder) or def.placeholder_text,
            padding=G.input_padding,radius=G.input_radius,font_size=G.input_font,fill="surfaceContainer"}
    end
    return ui.Column{id="field-"..def.slot,width="fill",gap=7,table.unpack(children)}
end

local function button(id,label,primary,request)
    local width=math.floor(utf8.len(label)*G.char_width+2*G.button_pad)
    return kit.Button{id=id,width=width,height=G.button_height,padding=0,radius=G.button_radius,justify="center",align="center",
        fill=primary and "primary" or "surfaceContainerHigh",activate_on_release=true,focus_on_pointer=false,aria_label=label,
        on_activate=ui.action("launcher.form",request),
        ui.Text{width=width,height=20,text=label,font_size=G.button_font,text_align="center",
            color=primary and "onPrimary" or "onSurface"}}
end

return ui.component("LauncherForm",{},function(_)
    local kind=get("launcher.form.kind")
    local def=FORMS[kind] or FORMS.reminder
    local fields={}
    for _,item in ipairs(def.fields) do fields[#fields+1]=field(item) end
    local description=def.description and ui.tr(def.description) or ""
    return ui.Box{id="form",width="fill",height="fill",
        ui.Row{position="absolute",left=0,right=0,top=G.top,justify="center",
            ui.Column{id="form-fields",width=G.column,gap=G.gap,
                ui.Text{id="form-title",width="fill",height=G.title_font+8,
                    text=def.title and ui.tr(def.title) or ui.bind("launcher.form.message"),font_size=G.title_font,color="onSurface"},
                ui.Text{id="form-description",width="fill",height=description=="" and 0 or 40,visible=description~="",text=description,
                    font_size=G.text_font,color="onSurfaceVariant"},
                table.unpack(fields)}},
        ui.Row{position="absolute",left=0,right=0,bottom=G.footer_bottom,justify="center",
            ui.Column{id="form-footer",width=G.column,gap=G.button_gap,
                ui.Text{id="form-error",width="fill",height=G.error_height,text=ui.bind("launcher.form.error"),
                    visible=ui.bind("launcher.form.has_error"),font_size=G.text_font,color="error"},
                ui.Row{gap=G.button_gap,
                    button("cancel",ui.tr("bead.launcher.form.cancel"),false,"cancel"),
                    button("submit",ui.tr(def.button),true,"submit")}}}}
end)
