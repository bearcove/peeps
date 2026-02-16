+++
title = "Roam Integration"
weight = 4
sort_by = "weight"
insert_anchor_links = "heading"
+++

Roam is a Rust-native RPC framework (separate project). It uses peeps throughout — all task spawning, channels, mutexes, timers, and futures in roam go through peeps wrappers. Beyond that, roam creates dedicated Request and Response nodes to track RPC lifecycles end-to-end.

This section documents how roam integrates with peeps.

Roam crates that use peeps: `roam-session`, `roam-shm`, `roam-stream`, `roam-tracing`, `roam-local`, `roam-telemetry`, `roam-http-bridge`.
