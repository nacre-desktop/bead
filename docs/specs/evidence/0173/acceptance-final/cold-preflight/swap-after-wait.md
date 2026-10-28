# Cold-arm preflight follow-up

The final actual-key attempt was refused before any compositor or wallpaper
picker process was launched because host swap use was at least 64 MiB. After a
20-second wait, `free -b` reported total swap 15,569,252,352 bytes, used
848,166,912 bytes, and free 14,721,085,440 bytes; MemAvailable was
8,913,297,408 bytes. `/proc/vmstat` showed `pswpin=291` pages and
`pswpout=176806` pages. The largest per-process swap users were unrelated
Steam web helpers (PIDs 17663, 17711, 17633, 17676) and Steam (PID 17426).
No task process remained, no process was signalled, and no swap reset was run.
