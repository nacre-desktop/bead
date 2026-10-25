| CPU phase | Shell median Δ MiB | Shell + cached chat median Δ MiB | Gate |
| --- | ---: | ---: | --- |
| audio_peek | -0.059 | +0.113 | pass |
| audio_peek_closed | +0.062 | +0.172 | pass |
| control_center_closed | +0.008 | +0.113 | pass |
| control_center_open | +0.039 | +0.195 | pass |
| dashboard_today_closed | +0.000 | +0.105 | pass |
| dashboard_today_open | +0.539 | +0.645 | FAIL |
| idle | -0.035 | -0.035 | pass |
| launcher_closed | -0.039 | +0.117 | pass |
| launcher_quick | +0.746 | +0.812 | FAIL |
| notifications_panel_closed | -0.047 | +0.078 | pass |
| notifications_panel_open | -0.027 | +0.098 | pass |
| sidebar_closed | -0.025 | +0.014 | pass |
| sidebar_open | -0.023 | +0.000 | pass |
| sidebar_reopened | -0.025 | -0.131 | pass |
| sidebar_reopened_closed | -0.025 | +0.080 | pass |

Unreclaimable = anon + shmem + DRM + SwapPss; five alternating pairs.
This snapshot series failed. Changing raw samples prevent a settled-memory claim.
