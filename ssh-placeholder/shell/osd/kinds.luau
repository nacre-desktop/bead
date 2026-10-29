-- SPDX-License-Identifier: GPL-3.0-only
-- On-screen display kinds (B0051, N0043): what the view knows about each `osd` event kind, as
-- data. `title` is a message id; `detail` is one of `state` (on/off), `fixed`, `by_label` (event label to id),
-- `label` (the event label, else `fallback`) or `fallback` (an id) -- empty when none applies. `level` kinds draw a bar and a percentage; `toggle`
-- kinds read `value` as on/off. A glyph is chosen by, in order: `muted`, the first `below`
-- limit above the value, the first `match` whose patterns occur in the label and device
-- (case-insensitive), `by_label`, then `default`.
return {
    volume={level=true,title="bead.osd.volume",detail={label=true,fallback="bead.osd.default-output"},
        glyph={muted="volume_off",below={{.010001,"volume_mute"},{.5,"volume_down"}},default="volume_up"}},
    output={title="bead.osd.output",detail={label=true,fallback="bead.osd.default-output"},
        glyph={match={{glyph="headphones",patterns={"headphone","headset","earbud"}},{glyph="bluetooth_audio",patterns={"bluez","bluetooth"}}},default="speaker"}},
    microphone={level=true,title="bead.osd.microphone",detail={label=true,fallback="bead.osd.default-input"},
        glyph={muted="mic_off",default="mic"}},
    brightness={level=true,title="bead.osd.brightness",detail={},
        glyph={below={{.5,"brightness_low"}},default="brightness_high"}},
    keyboard={level=true,title="bead.osd.keyboard",detail={fixed="bead.osd.keyboard-backlight"},glyph={default="keyboard"}},
    night_light={toggle=true,title="bead.osd.night-light",detail={state=true},glyph={default="bedtime"}},
    power_profile={title="bead.osd.power-mode",
        detail={by_label={["power-saver"]="bead.osd.power-saver",performance="bead.osd.performance"},fallback="bead.osd.balanced"},
        glyph={by_label={["power-saver"]="eco",performance="bolt"},default="balance"}},
    capslock={toggle=true,title="bead.osd.capslock",detail={state=true},glyph={default="keyboard_capslock"}},
}
