# 0278 Edge lip open continuity (B0278)

Owner report: hovering a left/right lip swells it correctly, but as its panel opens it
"turns black and super small" for a split second.

## Cause

The lip is drawn by the edge art callback from `LipState { progress, active }`. The only small,
unglowed look is `(progress 0, active false)`, the rest bar. On an external edge source the
Triggered cue sets the dwell progress to 0 (the cue is completed so the bar-hosted fill hides),
while the host panel reports open only a moment later, so the lip was redrawn as the rest bar
in that gap. The cue channel closing redraws it the same way.

## Fix

`EdgeState::opening` records the lip whose external open was just requested
(`external_lip_fire`). `lip_progress` then returns 1 for that lip (`drawn_progress`), so dwell
complete -> panel active is continuous. The mark clears when a draw sees the lip active, or
when a new Approach cue starts. A cue close or cancelled dwell without a fired open leaves it
unset and still returns the lip to rest. Panel close returns to rest through `active`.

The bar-hosted cue (`publish_cue`) still hides at fire; only the lip's own drawing holds.
Test: `edges::tests::pending_open_holds_the_completed_dwell_instead_of_the_rest_bar`.
Limit: if a requested open never happens the lip stays swollen until the next Approach.
