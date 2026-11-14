# 0275 Clock resync without the system bus (B0275)

Hosts may run the services with `Config { bus: false }`; the clock must still update at once
after suspend/resume, a wall-clock step and a timezone switch. Before this unit the clock worker
slept on a monotonic condvar timeout, so after resume it showed the old time until that timeout
ran out (up to 60 s), and a timezone switch reached it only through timedate1 on the system bus
(and not even then on glibc: `localtime_r` never re-reads `/etc/localtime` without `tzset`).
Hosts worked around resume by taking and dropping a `clock_seconds_lease()`.

## Design (Linux)

- The clock worker (`bus::run_clock`) arms a `CLOCK_REALTIME` timerfd with
  `TFD_TIMER_ABSTIME | TFD_TIMER_CANCEL_ON_SET` for the next minute (second under a lease)
  boundary and polls it. A wall-clock step (`clock_settime`, an NTP step) or resume, which the
  kernel reports as a clock set, cancels the wait and the worker recomputes at once.
- The same poll holds an inotify watch on `/etc` (create, delete, move from/to, close-write) and
  reacts to entries named `localtime`: a zone switch renames a new symlink over it. The worker
  then calls `tzset()` and republishes. With `TZ` set there is no watch.
- Lease changes and `Desktop::stop` wake the worker by re-arming the timer to a past deadline
  under the `clock_wait` lock, so a wake cannot fall between the worker's checks and its poll.
  A backward step between reading the wall clock and arming is checked right after arming.
- Every wake recomputes; `Shared::change` drops a snapshot equal to the published one. The
  minute-slot and wall/monotonic drift bookkeeping of the monotonic wait is deleted.
- On Linux the bus no longer refreshes the clock on `PrepareForSleep(false)` or timedate1
  signals; the worker covers both with or without the bus.

Elsewhere, and on Linux if the kernel refuses a timerfd, the worker keeps the condvar wait and
the bus refreshes as before. No public API change: hosts can delete their resume lease kick.

Tests: `any_clock_wake_republishes_the_wall_clock_at_once` (a wake with a stale snapshot and no
lease change republishes at once, the readiness a cancelled wait gives; a real cancel needs
`CAP_SYS_TIME`), `zone_watch_reports_only_localtime_entries` (real inotify in a temp directory),
and the existing tick, lease, stop and `clock_private_bus` tests.

## Limits

- Edits of the zone file `/etc/localtime` points at (a tzdata update) are not watched; they
  show after the next zone switch or a restart.
- musl's `tzset` caches by name and does not re-read `/etc/localtime`; a zone switch there
  needs a restart.
