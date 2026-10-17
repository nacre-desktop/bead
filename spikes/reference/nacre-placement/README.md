# Nacre placement reference

Frozen GPL-3.0-only product references from Nacre e0a81ce7. Exact source hashes
are in source.json. No reference code belongs to a permissive framework crate.

`spikes/placement-reference.py` runs layout.js and contour.js in real Quickshell
with an offscreen backend, private HOME/XDG and a private session bus. It imports
FrameRim with only its module paths redirected to fixed palette/settings stubs.
Motion is disabled; default frame geometry and the committed palette are fixed.
FrameLips supplies an empty rimHandles list in this revision.

The eleven geometry cases include all current popouts, both side attachments,
the requested top launcher, the published bottom attachment and corner limits.
The owner explicitly requests TOP for Bead's launcher; the published QML host
currently chooses bottom. Both exercise the same canonical attached() function.

The 2880×1800 placement masks use 1920×1200 logical coordinates at scale1.5.
Each pair PNG has QML on the left and Bead on the right. They check placement
and joined frame shape, not content parity or hardware state. Actual Luau body
sizes are checked; existing populated content goldens remain separate. Paths
and logical origins compare exactly. Raster comparisons allow small renderer
antialiasing differences (channel4,0.1% of output pixels); Bead self goldens use
the harness default. No performance claim comes from these images.

Regenerate the Qt side from the isolated checkout:

```sh
nice -n 10 python3 spikes/placement-reference.py \
  --output shell/tests/fixtures/placement-qt.json \
  --screenshots shell/tests/goldens/placement --scale 1.5
```

Generate Bead masks/pairs with BEAD_UPDATE_GOLDENS=1 and the
`placement::pixels::` nacre-shell test. Normal CI only compares committed files.
