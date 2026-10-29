#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Optional capture tool: fonttools==4.60.1 in an isolated environment, not CI/runtime."""
import argparse
import hashlib
from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

parser = argparse.ArgumentParser()
parser.add_argument("source")
parser.add_argument("output")
args = parser.parse_args()
with open(args.source, "rb") as source:
    if hashlib.sha256(source.read()).hexdigest() != "95b24392bb49efd1bc3e92cff4e2452ad094461bab7c97e7d8723fab97e330ca":
        raise SystemExit("source font is not the pinned Google Material Symbols revision")
font = TTFont(args.source)
font = instantiateVariableFont(font, {axis.axisTag: axis.defaultValue for axis in font["fvar"].axes}, inplace=True)
options = subset.Options()
options.layout_features = ["*"]
options.name_IDs = ["*"]
options.name_legacy = True
options.name_languages = ["*"]
subsetter = subset.Subsetter(options=options)
subsetter.populate(text="close expand_less expand_more chevron_right star desktop_windows volume_up volume_off mic mic_off brightness_high battery_full bluetooth wifi network_wifi power_settings_new search settings keyboard notifications info warning")
subsetter.subset(font)
for record in font["name"].names:
    text = {1: "Bead Test Symbols", 2: "Regular", 4: "Bead Test Symbols", 6: "BeadTestSymbols", 16: "Bead Test Symbols", 17: "Regular"}.get(record.nameID)
    if text:
        record.string = text.encode(record.getEncoding())
font.save(args.output)
