# Dashboard extras preview data

Artificial states of the published `notifications`, `bluetooth` (Devices page),
`audio` and `network` topics for the B0024 read-only extra pages. They are kept
apart from `../dashboard` because that tree is a frozen B0002 control input.
With `--dashboard-fixtures DIR` an extra page falls back to the sibling
`dashboard-extras` directory. Tests validate them with the pinned protocol types.

## B0027 additions

`focus`, `clipboard`, `screenshots`, `ai-usage` and `brain` are artificial states of the
N0034 topics `dashboard.focus`, `clipboard`, `screenshots`, `ai.usage` and `brain.recent`;
`settings.json` is the `settings.store.dashboard-extras` store (tab enable/order). The focus
deadline is far in the past on purpose, so the preview shows `00:00` rather than a clock reading.
`player.json` is an artificial `dashboard.player` state for the Media page's playback card
(`system` metrics are exercised with an extended copy of the frozen system fixture in tests).
