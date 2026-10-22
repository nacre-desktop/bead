# Dashboard extras preview data

Artificial states of the published `notifications`, `bluetooth` (Devices page),
`audio` and `network` topics for the B0024 read-only extra pages. They are kept
apart from `../dashboard` because that tree is a frozen B0002 control input.
With `--dashboard-fixtures DIR` an extra page falls back to the sibling
`dashboard-extras` directory. Tests validate them with the pinned protocol types.
