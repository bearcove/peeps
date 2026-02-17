# Repository Agent Notes

## JavaScript/TypeScript Package Manager

- Use `pnpm` for all frontend dependency and script workflows in this repository.
- Do not use `npm`.
- Standard commands:
  - `pnpm install`
  - `pnpm build`
  - `pnpm test`

## Documentation Style

Documentation in this repository is written by a human with a strong voice. Preserve that voice.

When the user provides copy, keep original cadence and phrasing. Do not "smooth" or rewrite by default. Only fix obvious typos unless explicitly asked to do heavier editing.

The tone is warm and explanatory, but never salesy. Do not market the project, do not brag, and do not compare against other open source projects unless explicitly requested. Describe what the system does, what it costs, and what tradeoffs it implies.

Write from the perspective of someone with deep, cross-cutting industry experience. This codebase is part of a broader ecosystem; when relevant, connect concepts to sibling crates and how they work together.

Default docs format is narrative long-form, not bullet-heavy "AI slop". Use bullets sparingly. Diagrams, tables, and images are welcome when they clarify hard concepts; suggest adding visuals when they would materially improve comprehension.

Across docs pages, keep the same framing discipline: state the problem plainly, state tradeoffs honestly, and state the payoff without hype.
