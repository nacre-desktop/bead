-- SPDX-License-Identifier: GPL-3.0-only
-- Control-center tile registry (B0049). A tile is data: {id, kind, size, icon, label, source, view|page,
-- default}. `source` names a data source the host publishes in the `cc.src` store (value, state and which
-- verbs it accepts); `view` opens another panel; `page` opens an in-panel detail page. A tile whose source
-- or view is absent is unavailable and only listed in the catalog with its `ask` (the missing contract), so
-- it appears the moment the host publishes it. Add or reorder entries here: no Rust change is needed for a
-- tile over an existing source. A new source is one entry in `control_center.rs`.
local layout={
    width=424,
    -- tiles per row and row height for each size; heights are multiples of the shared scale
    sizes={full={per_row=1,height=56},half={per_row=2,height=66},third={per_row=3,height=76}},
}
local tiles={
    {id="volume",kind="slider",size="full",icon="volume_up",label="bead.panel.control-volume",source="audio.output",default=true},
    {id="brightness",kind="slider",size="full",icon="brightness_6",label="bead.panel.control-brightness",source="brightness.level",ask="brightness.level",default=true},
    {id="mic-volume",kind="slider",size="full",icon="mic",label="bead.panel.control-mic-volume",source="audio.input",ask="audio.set_volume",need="set"},
    {id="keyboard",kind="slider",size="full",icon="keyboard",label="bead.panel.control-keyboard",source="keyboard.backlight",ask="keyboard.backlight"},
    {id="network",kind="toggle",size="half",icon="wifi",label="bead.panel.control-network",source="network.wifi",view="network",default=true},
    {id="bluetooth",kind="toggle",size="half",icon="bluetooth",label="bead.panel.control-bluetooth",source="bluetooth.power",view="bluetooth",default=true},
    {id="airplane",kind="toggle",size="half",icon="flight",label="bead.panel.control-airplane",source="airplane.mode",ask="airplane.mode"},
    {id="vpn",kind="toggle",size="half",icon="vpn_key",label="bead.panel.control-vpn",source="vpn.state",ask="vpn topic"},
    {id="battery",kind="link",size="half",icon="balance",label="bead.panel.control-battery",source="power.profile",view="battery",default=true},
    {id="awake",kind="toggle",size="half",icon="coffee",label="bead.panel.control-awake",source="stay-awake.state",ask="stay-awake.set",default=true},
    {id="dnd",kind="toggle",size="half",icon="do_not_disturb_on",label="bead.panel.control-dnd",source="notifications.dnd",page="bead.panel.control-dnd-text",default=true},
    {id="mic",kind="toggle",size="half",icon="mic",label="bead.panel.control-mic",source="audio.input",ask="audio.set_mute",need="toggle"},
    {id="night",kind="toggle",size="half",icon="nightlight",label="bead.panel.control-night",source="night-light.state",ask="night-light.set",default=true},
    {id="appearance",kind="toggle",size="half",icon="palette",label="bead.panel.control-appearance",source="appearance.mode",ask="appearance.set_mode",default=true},
    {id="screenshot",kind="button",size="third",icon="screenshot",label="bead.panel.control-screenshot",source="screenshot.capture",ask="screenshot.capture"},
    {id="record",kind="toggle",size="half",icon="videocam",label="bead.panel.control-record",source="recorder.state",ask="recorder.toggle"},
    {id="picker",kind="button",size="third",icon="colorize",label="bead.panel.control-picker",source="color-picker.pick",ask="color-picker.pick"},
    {id="clipboard",kind="button",size="third",icon="content_paste",label="bead.panel.control-clipboard",view="dashboard-clipboard"},
    {id="updates",kind="button",size="third",icon="system_update",label="bead.panel.control-updates",view="dashboard-system"},
    {id="displays",kind="button",size="third",icon="desktop_windows",label="bead.panel.control-displays",source="displays.layout",ask="displays topic"},
    {id="wallpaper",kind="button",size="third",icon="wallpaper",label="bead.panel.control-wallpaper",source="settings.window",ask="bead.window open"},
    {id="next-wallpaper",kind="button",size="third",icon="skip_next",label="bead.panel.control-next-wallpaper",source="wallpaper.advance",ask="wallpaper.advance"},
    {id="notifications",kind="button",size="third",icon="notifications",label="bead.panel.control-notifications",view="dashboard-notifications"},
}
local by_id={}
for _,t in ipairs(tiles) do by_id[t.id]=t end
return {layout=layout,tiles=tiles,by_id=by_id}
