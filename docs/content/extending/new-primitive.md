+++
title = "Adding a New Instrumented Primitive"
weight = 1
insert_anchor_links = "heading"
+++

Add new primitives by following the same causal contract used everywhere else.

## Checklist

1. Add/choose a node kind that represents the runtime entity.
2. Provide `diagnostics`-enabled and disabled implementations.
3. Register node on creation; remove on drop/completion.
4. Emit `Needs` while blocked; remove when unblocked.
5. Ensure stack/scoping integration so `Touches` can be inferred.
6. Include timing and source-location metadata that helps debugging.
7. Re-export from the public API.

## Rule of thumb

Prefer a small, stable semantic surface:

- identity (unique node ID),
- lifecycle (create/update/remove),
- causality (edges),
- observability (timing + location).

Avoid overfitting docs or schema to current field names. Keep behavior stable; let details iterate in code.
