# Core dashboard QML coverage

Reference: Nacre `5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57`, tree
`nacre/shell/modules/dashboard`. The exact tree contains **41 QML files** and
two JavaScript helpers (`media.js`, `overview.js`); the brief's QML count is
correct. This is a scope inventory, not a claim of pixel parity for optional
Nacre pages. Reserved protocol schemas do not imply runtime providers.

| Source component | User behavior | Bead equivalent | Test proof | Disposition |
| --- | --- | --- | --- | --- |
| `NacreAlbumHalo.qml` | Animated album/spectrum halo | — | — | deferred optional |
| `NacreChamberGauge.qml` | Circular system metric gauge | `shell/dashboard/system/main.luau` metric area | `unknown_weather_updates_and_hardware_values_never_become_zero` | blocked by published runtime provider |
| `NacreCompactMetric.qml` | Dense dashboard metric | System metric area; unknown readings stay unavailable | `unknown_weather_updates_and_hardware_values_never_become_zero` | blocked by published runtime provider |
| `NacreDashboardCustomize.qml` | Dashboard customization view | — | — | deferred optional |
| `NacreDashboardCustomizeSection.qml` | Customization section grouping | — | — | deferred optional |
| `NacreDashboardExtrasOptions.qml` | Configure optional cards | — | — | deferred optional |
| `NacreDashboardHero.qml` | Main summary/hero content | `shell/dashboard/components.luau`, page layouts | `dashboard_pages_have_deterministic_pixels_and_only_navigation_actions` | current-core covered |
| `NacreDashboardMetric.qml` | Shared metric display | `shell/dashboard/components.luau`, System/Weather pages | `dashboard_four_tabs_have_complete_state_defaults` | current-core covered |
| `NacreDashboardNavigation.qml` | Switch core dashboard tabs | `shell/dashboard/main.luau` | `dashboard_navigation_routes_every_core_tab` | current-core covered |
| `NacreDashboardNetworkCard.qml` | Network status card | — | — | deferred optional |
| `NacreDashboardPanel.qml` | Dashboard panel and page host | `shell/dashboard/main.luau` | `dashboard_pages_have_deterministic_pixels_and_only_navigation_actions` | current-core covered |
| `NacreDashboardSparkline.qml` | Historical metric graph | — | — | deferred optional |
| `NacreDashboardSummary.qml` | Compact status summary | Today/Weather page summary fields | `dashboard_four_tabs_have_complete_state_defaults` | current-core covered |
| `NacreDashboardToggleRow.qml` | Toggle optional features/settings | — | — | deferred optional |
| `NacreDashboardVideo.qml` | Video/photo background | — | — | deferred optional |
| `NacreFocusPage.qml` | Focus mode and mutations | — | — | deferred optional |
| `NacreGpuMemory.qml` | GPU memory detail | Unknown GPU readings remain unavailable | `unknown_weather_updates_and_hardware_values_never_become_zero` | blocked by published runtime provider |
| `NacreMediaPage.qml` | Lyrics and palette enrichment | `shell/dashboard/media/main.luau` | `dashboard_four_tabs_have_complete_state_defaults`; `dashboard_disconnect_keeps_last_good_stale` | current-core covered |
| `NacreMiniPlayer.qml` | Playback controls and progress | — | — | deferred optional |
| `NacreSystemPage.qml` | Process/filesystem rows and updates | `shell/dashboard/system/main.luau` | `dashboard_long_rows_scroll_without_clipping`; existing system projection tests | current-core covered |
| `NacreTodayPage.qml` | Clock/date, calendar agenda and weather summary | `shell/dashboard/today/main.luau` | `dashboard_four_tabs_have_complete_state_defaults`; `today_calendar_navigation_and_agenda_scrolling_use_real_input` | current-core covered |
| `NacreWeatherPage.qml` | Current, hourly and daily weather | `shell/dashboard/weather/main.luau` | `dashboard_long_rows_scroll_without_clipping`; deterministic page pixels | current-core covered |
| `NacreWeatherSky.qml` | Weather photography/sky effects | — | — | deferred optional |
| `extras/NacreAiUsageExtra.qml` | AI usage card | — | — | deferred optional |
| `extras/NacreBrainExtra.qml` | Brain card | — | — | deferred optional |
| `extras/NacreClipboardExtra.qml` | Clipboard card | — | — | deferred optional |
| `extras/NacreDevicesExtra.qml` | Device card | — | — | deferred optional |
| `extras/NacreExtraFrame.qml` | Shared optional-card frame | — | — | deferred optional |
| `extras/NacreExtraRow.qml` | Shared optional-card row | — | — | deferred optional |
| `extras/NacreFocusExtra.qml` | Focus card and actions | — | — | deferred optional |
| `extras/NacreNetworkExtra.qml` | Network card | — | — | deferred optional |
| `extras/NacreNotificationsExtra.qml` | Notification card | — | — | deferred optional |
| `extras/NacreScreenshotsExtra.qml` | Screenshot card | — | — | deferred optional |
| `media/NacreAlbum.qml` | Album artwork rendering/decoding | — | — | deferred optional |
| `media/NacreMediaButton.qml` | Player transport action | — | — | deferred optional |
| `media/NacreMediaSlider.qml` | Seek/volume control | — | — | deferred optional |
| `media/NacrePlayerAudioLevel.qml` | Player audio-level visualization | — | — | deferred optional |
| `overview/NacreCalendarCard.qml` | Calendar summary card | Today agenda/calendar | `today_calendar_navigation_and_agenda_scrolling_use_real_input` | current-core covered |
| `overview/NacreHostCard.qml` | Host summary/metrics | — | — | deferred optional |
| `overview/NacreMediaCard.qml` | Current-player card | No player/MPRIS source is advertised | — | blocked by published runtime provider |
| `overview/NacreOverviewCard.qml` | Optional overview-card container | — | — | deferred optional |

The current Bead media page is enrichment-only: it does not discover a player,
expose transport controls, or decode album art. System process/disk and Updates
views can render explicit fixtures, but the reserved core schemas are not hosted
topics; live daemon-backed data remains unavailable until a provider is
published. Nacre's `media.js` and `overview.js` are helper scripts, not QML rows;
they are outside the requested 41-file inventory and no ranking or player
behavior is ported here.

The lifecycle test holds each fake source alive, subscribes through an actual
page `Runtime`, explicitly drops that Runtime, then publishes again and proves
the retired page callback is silent. It covers Today calendar/weather/clock,
Weather, Media and System/Updates. This proves Rust subscription ownership, not
GPU/allocator memory reclamation or the elapsed close grace. The outer Bead `close_panel()` timer, epoch check and reopen token remain wired
in `crates/bead-wayland/src/panel.rs` at Bead base
`04c492a636593e47ccbfc1d4f45ca25126fca307`; Nacre reference is
`5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57`. The separate
`service_views_tests::panel_retirement_releases_launcher_mirror_but_bar_does_not`
test concerns launcher/bar ownership, not the timer. No new timer or residency
measurement was run under B0001.
