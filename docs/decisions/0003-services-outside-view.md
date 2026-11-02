# ADR 0003 — Services outside the view

Status: accepted.

Views consume typed snapshots and emit declared actions through host-provided interfaces. Service connections, external state, and application policy stay outside view code. The framework supplies reusable service adapters where they are general; the host selects services and owns product protocols and routes.

This boundary keeps views deterministic enough to validate and reload atomically, and prevents framework crates from acquiring application daemon or protocol dependencies.
