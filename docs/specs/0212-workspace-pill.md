# B0212 Workspaces pill alignment and gallery samples

Status: candidate; headless/full CI and private parity captures pending host availability.

- Overflow used a fixed 28 px text box while app icons used `centered_text`'s
  font-sized line boxes. Row alignment centred the boxes, but text paints from
  their tops. The badge now uses the same shared helper (including the existing
  text-family lift); no scale-specific offset, padding or snapping change.
- An empty workspace model made the gallery's horizontal Row just 10 px wide
  and 34 px tall: a vertical-looking empty capsule. The app now aliases its
  workspace binding to three samples, one active and one with a window icon.
  Both sizing and Repeat resolve that same store. The alias survives the editor
  host clearing the real workspace model; live bar/editor data is unchanged.
- The generic pill container still previews empty, as required by B0208.

Proof added (not executed on the laptop):
- `service_views_tests::workspace_overflow_and_app_icons_share_a_vertical_centre_at_both_scales`:
  +2 and three app icons have line-box centres within 0.5 logical px at 1 and 1.5.
- `reload::widget_editor_tests::the_gallery_workspaces_preview_has_horizontal_samples_and_one_active_workspace`:
  gallery frame contains three ordered, vertically aligned workspace labels, a
  window icon and exactly one active capsule, even with an empty real model.

Existing goldens are unchanged: workspace icon-mode geometry is untouched, and
the bar parity fixture has no overflow badge. Run the full existing suites before
acceptance. Paired QML/Bead captures remain unverified until the test host returns.
No new always-on state; sample records live only in the on-demand Widgets app.
Replaced the fixed-height badge declaration; no duplicate rendering path remains.
