# B0102 — Dashboard owner fixes

The B0096 status-pill runtime sent bar-only state into companion surfaces on
pointer entry/exit. The dashboard rejected `status.hovered`, keeping its old UI.
Pill publishing now belongs only to bar sessions. A runtime regression loads all
13 dashboard pages and exercises pointer, timer, selection, dismissal and reload.

Today follows QML's wallpaper hero and three-column calendar, host/metrics and
media summary. Its Luau modules own geometry, typography and transport controls.
Published `dashboard.system` and `wallpaper.active` sources are leased only by
the open Today page; the existing descriptor-pinned wallpaper decoder produces
display-sized pixels in its short-lived worker. Closing releases sources and art.
No new framework primitives or temporary duplicate paths.

The former agenda card was removed because QML's Today summary has a month
calendar, not an agenda. The private reference profile has no configured calendar
file; no ICS content or private configuration was copied into the repository.
Calendar navigation remains covered with real input. Today uses replaceable shell
theme values in `style.luau`, with canvas dimensions declared in its behavior.

## Validation / outstanding gate

The urgent fix is published separately as `ac194d7`. The Today implementation is
an unpublished candidate. Its 57 dashboard tests pass remotely, including every
page's pointer/timer lifecycle, visible fallback clock, host/storage projection,
poster reuse, disconnect/close release and source retirement. Eight affected
1×/1.5× goldens were regenerated and visually inspected.
Full CI-equivalent passed on `6c65d8c` in a remote Arch container with Rust 1.99.0:
[commands](evidence/0102/check.sh), [results](evidence/0102/check-results.txt).
Arch setup additionally needs swaybg, wlr-randr, DejaVu/IBM Plex fonts and removal
of Sway's file capability for the root container's private smoke session.

Paired QML/Bead captures and relative unreclaimable-memory measurements remain
blocked by another unit holding the private desktop capture lock. RMSE is missing,
not zero. Matching control/candidate CPU builds and the private fixture harness
are prepared, but this candidate must not be treated as accepted parity.

Known gaps awaiting comparison: Today wallpaper motion and wallpaper actions,
home-time/palette captions, the media progress ring/audio halo; full Media player
layout/options, Weather forecast/details and System card layout. The Today album
is circular, uses the art source and accessible icon controls; its compact artist
line omits the album as in QML. No per-second Today timer was added.
