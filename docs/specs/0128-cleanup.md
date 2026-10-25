# B0128 — old artifact cleanup audit

Audit date: 2026-10-25. Scope: Bead and the Nacre `origin/main` worktree. Only expired or demonstrably unreferenced artifacts were eligible for deletion. No product-code candidates met both tests.

| Item | Added by | Expiry condition | Met today? | Action |
| --- | --- | --- | --- | --- |
| QML duplicates, app copies and parity paths (B0019–B0034, B0043–B0048, B0059–B0064, B0084, B0092) | listed unit | trusted Bead switch, owner soak, or explicit QML retirement plus grace period | No; QML deletion waits for soak | Keep; deletion is outside this unit. |
| Registry/i18n and daemon compatibility copies (B0032, B0045, B0062–B0063; N0017, N0024, N0026–N0039, N0043–N0047, N0050–N0069) | listed unit | published/consumed replacement and the stated 7/14/30-day window | No, or still referenced by current shell/protocol paths | Keep; Nacre protocol aliases remain in implementation and tests. |
| Bead rollback, switch and local state copies (B0029, B0033, B0048, B0059, B0068, B0084) | listed unit | trusted switch/first use and 7/30-day grace; shared pearl store must be published and consumed | No; QML is retained and no shared pearl store exists | Keep. |
| Capture/review snapshots (B0098, B0103, B0105, B0107, B0111) | listed unit | final capture/review trusted, then 7 days | Not proven for every item | Keep; evidence remains pinned to its source. |
| Memory measurement lanes `spikes/b0081-gate.py`, `spikes/b0085-gate.py`, `spikes/b0085-transient.py` | B0081/B0085 | first laptop capture accepted +14 days; next profile/memory unit accepted +14 days | No; B0085 is 2026-10-24 and its follow-up condition is later | Keep. |
| Other unit-named Bead measurement spikes: `b0002-arm.py`, `b0003-arm.py`, `b0030-memory.py`, `b0047-frame.py`, `b0048-pearls.py`, `b0051-gates.py`, `b0052-app.py`, `b0059-welcome.py`, `b0065-smaps.py`, `b0090-previews.py` | matching B-unit | no explicit elapsed expiry; each is referenced by a spec, test or another harness | No; references remain | Keep. |
| B0065 `code_pages` module and B0069 `b0069-codepages.py` | B0065/B0069 | superseded by B0081 | Already absent before this audit | No change; grep found no implementation leftovers. |
| Nacre pass-(a) Python/rollback/settings duplicates | N0018+ queue | pass (a), one week after daemon mode is stable (2026-10-21) | No; earliest date is 2026-10-28 and the owner-approved audit is pending | Keep until scheduled pass. |
| Nacre QML shell and its harnesses | N0073 / owner gate | Bead core-switch soak, then owner-approved QML retirement + 7 days | No | Keep; deletion explicitly out of scope. |
| Nacre unit-named measurement spikes | — | none found in `spikes/` or `tools/` | N/A | No candidates. |

Static dead-code review: no unused Luau module was established (widget modules are dynamically required by the registry); Rust CI's denied warnings cover private dead code. Nacre's `bluetooth.set_enabled` alias is still present in protocol schemas, fixtures, and helper dispatch, so removing it is a breaking change and is not authorized by this audit.

## Large committed evidence (>1 MiB)

No evidence was deleted. Bead has 75 files over 1 MiB; Nacre has no `docs/specs/evidence/` directory. Bead paths:

```text
0077/full/{idle-qml.jpg,idle-bead.jpg,palette-qml.jpg,palette-bead.jpg,keybindings-qml.jpg,keybindings-bead.jpg}
0078/{qml-idle.jpg,bead-idle.jpg,qml-weather-hero.jpg,bead-weather-hero.jpg}
0079/full/{idle-qml.jpg,idle-bead.jpg,palette-qml.jpg,palette-bead.jpg,palette-hover-qml.jpg,palette-hover-bead.jpg,palette-pressed-qml.jpg,palette-pressed-bead.jpg,keybindings-qml.jpg,keybindings-bead.jpg,keybindings-hover-qml.jpg,keybindings-hover-bead.jpg,keybindings-pressed-qml.jpg,keybindings-pressed-bead.jpg}
0083/idle.png
0092/{palette.png,palette-typed.png,palette-brain.png}
0093/{menu-pick-qml.jpg,menu-pick-bead.jpg,menu-pick-hover-qml.jpg,menu-pick-hover-bead.jpg,menu-input-qml.jpg}
0094/{control-center-before.png,control-center-after.png,control-audio-before.png,control-audio-after.png}
0095/{control-audio-before.png,control-audio-after.png,control-audio-muted-before.png,control-audio-muted-after.png,control-network-wifi-before.png,control-network-wifi-after.png,control-network-scanned-before.png,control-network-scanned-after.png,popout-audio-before.png,popout-audio-after.png,popout-audio-muted-before.png,popout-audio-muted-after.png,popout-network-wifi-before.png,popout-network-wifi-after.png,popout-network-scanned-before.png,popout-network-scanned-after.png}
0098/{sidebar-before-pair.jpg,sidebar-pair.jpg,sidebar-pinned-pair.jpg,sidebar-hover-pair.jpg,sidebar-pressed-pair.jpg,sidebar-conversation-before-pair.jpg,sidebar-conversation-pair.jpg,settings-before-pair.jpg,settings-pair.jpg,settings-scroll-before-pair.jpg,settings-scroll-pair.jpg}
0100/wallpaper-activated-pair.jpg
0102/{idle-whole-window.jpg,dashboard-0-whole-window.jpg,dashboard-1-whole-window.jpg,dashboard-2-whole-window.jpg,dashboard-3-whole-window.jpg,dashboard-hover-whole-window.jpg,dashboard-pressed-whole-window.jpg}
0109/{wallpaper-travel.jpg,wallpaper-travel-series.jpg,wallpaper-travel-burst-series.jpg}
```

Proposal for lead: retain paired original captures as the audit source, but use lossless thumbnails for routine Markdown previews; store any future raw series outside Git with a stable checksum/manifest and keep only the manifest plus representative samples in the repository. No bulk recompression/deletion was authorized here.

Product lines changed: 0 added, 0 removed. This audit adds documentation only.
