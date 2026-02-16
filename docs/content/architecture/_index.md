+++
title = "Architecture"
weight = 2
sort_by = "weight"
insert_anchor_links = "heading"
+++

peeps has three crates. `peeps` is the instrumentation library linked into your application. `peeps-types` defines the shared data model. `peeps-web` is the server and web frontend. All instrumentation is feature-gated behind `diagnostics` — when the feature is off, every wrapper compiles to a zero-cost pass-through.
