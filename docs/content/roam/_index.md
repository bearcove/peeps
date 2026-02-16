+++
title = "Roam Integration"
weight = 4
sort_by = "weight"
insert_anchor_links = "heading"
+++

Roam is instrumented with peeps end-to-end. Standard runtime primitives use peeps wrappers, and RPC lifecycle adds request/response-specific nodes and links.

This section focuses on cross-process causality intent, not protocol field inventory.

Roam crates that use peeps: `roam-session`, `roam-shm`, `roam-stream`, `roam-tracing`, `roam-local`, `roam-telemetry`, `roam-http-bridge`.
