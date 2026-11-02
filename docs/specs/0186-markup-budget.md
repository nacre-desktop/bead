# Markup execution budget

Markup execution uses a bounded per-thread CPU-time budget so scheduler pauses and system sleep do not count as view computation. The host may configure the limit; the framework reports an overrun with source context and keeps the last valid UI generation active.

A transient execution failure may be retried after a subsequent change. A source error remains visible until a successful reload. Recovery must not expose a partially built generation.
