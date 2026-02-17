# Tone Guide

This file is the source of truth for documentation voice in this repository.

## Core voice

Write like an experienced engineer explaining a real debugging problem to another experienced engineer.

The voice is warm, explanatory, direct, and candid. It is not sales copy.

## What this should sound like

Start from lived reality, not abstract taxonomy.

Good: "your async app is stuck, you attach a debugger, and all threads are parked."

Less good: "this system consists of nodes, edges, and events."

Explain execution mechanics step by step so the reader can replay what happened mentally.

Be explicit about tradeoffs, costs, limitations, and uncertainty. It is okay to say the tool is hard to build, hard to use, and still in flux.

Use first-person ownership naturally when it is true to the author's voice.

Transition to structure after motivation. Define model terms only after the reader understands the problem they solve.

## Writing constraints

Do not smooth, sanitize, or "professionalize" user-provided copy unless explicitly asked.

Default behavior for edits is typo fixes only, preserving cadence and phrasing.

Do not market, brag, or do competitive posturing. Comparisons are allowed only to explain concrete differences in debugging model.

Narrative long-form is the default shape. Use bullets, tables, diagrams, and images when they materially improve understanding.

## Framing checklist

Across docs pages, keep this sequence when possible:

1. Problem: what hurts in practice
2. Why existing mental model fails here
3. Tradeoff: what we instrument and why
4. Payoff: what new visibility this gives
5. Limits: what is still hard
