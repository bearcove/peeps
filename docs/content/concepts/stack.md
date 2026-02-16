+++
title = "Causality Stack"
weight = 4
+++

The causality stack is task-local execution context: "what node is currently running in this task?" It enables automatic causal linking without global mutable state.

## Why it exists

- Without it, wrappers would need explicit parent IDs everywhere.
- With it, nested polls/interactions can inherit context automatically.
- It makes `Touches` edges low-friction and consistent.

## Mental model

Think of each poll cycle as entering a scope:

1. Enter scope for the current node.
2. Do work and touch other resources/futures.
3. Emit causal edges from current scope to touched entities.
4. Exit scope.

In practice, this creates a chain from high-level handler/task nodes down to the resources they wait on.
