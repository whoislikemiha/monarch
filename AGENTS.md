# Monarch — Agent & Contributor Guide

This project's full agent, build, and convention guide lives in **[CLAUDE.md](./CLAUDE.md)** — the canonical developer document: the architecture map, key files, build & dev commands, code patterns, the design system, and the rules & gotchas. `AGENTS.md` is kept as a stable entry point for tools that look for it by name; rather than duplicate CLAUDE.md, it only carries the repo conventions below. For anything not covered here, read CLAUDE.md.

## Conventions

- **Branches** — `{github-username}/mon-{N}-{slug}`, one branch per tracked issue, branched off `master`.
- **Commits** — conventional commits scoped to the issue: `type(mon-N): description`, with types `feat`, `fix`, `refactor`, `perf`, `chore`, `docs`. Commit often — one logical change per commit — and rebase onto `master` before merging.
- **Design history lives in [`thoughts/`](./thoughts/)** — research plans in `thoughts/plan/MON-{N}.md`, implementation notes in `thoughts/impl/MON-{N}.md`, longer-form design docs in `thoughts/design/`. Reading a ticket's plan and impl note is the fastest way to understand why a feature is shaped the way it is.
- **Keep docs alive** — changes that affect architecture, data model, protocol, or conventions update [CLAUDE.md](./CLAUDE.md) / [ONBOARDING.md](./ONBOARDING.md) in the same PR. Stale docs are worse than no docs.
