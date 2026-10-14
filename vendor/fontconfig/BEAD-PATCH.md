# fontconfig 0.11.0 owned-match patch

MIT source from the published fontconfig 0.11.0 crate, upstream revision
`cb7d366449b5c59e72b46d6da8b3e05824799c70` (fontconfig subdirectory).
Preserved LICENSE and attribution; Cargo.toml sets publish=false and README
whitespace is normalized.

FcFontMatch returns a new owned pattern. The upstream font_match wrapper passes
it to from_pattern, which adds a reference appropriate for borrowed patterns.
Drop destroys only one reference, retaining the newly returned pattern forever.
font_match now adopts that reference directly; from_pattern's borrowed-pattern
behavior is unchanged. Native bar text updates exposed the leak when selecting
fonts after a title arrives. Before patch, rendered tray churn grew allocated
native heap while Luau/tasks/images/subscriptions remained stable.
