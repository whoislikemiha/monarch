# Monarch — Agent & Contributor Guide

This project's full agent, build, and convention guide lives in **[CLAUDE.md](./CLAUDE.md)** — the canonical developer document: the architecture map, key files, build & dev commands, code patterns, the design system, and the rules & gotchas. `AGENTS.md` is kept as a stable entry point for tools that look for it by name; rather than duplicate CLAUDE.md, it only carries the workflow conventions below. For anything not covered here, read CLAUDE.md.

## Workflow

Linear is the source of truth for work items. GitHub is for code and PRs. Every non-trivial change starts with a Linear ticket and ends with a PR linked back to it.

### Linear-first development

- **No ticket, no work.** If a task doesn't have a Linear issue, create one before starting. Use `/linear-to-plan` for the full flow, or create one directly for smaller items.
- **One ticket = one branch = one PR.** This is the atomic unit of work. If a ticket's scope grows beyond a single coherent PR, split it — create sub-tasks or new tickets for the spun-off work rather than bloating the original.
- **Keep tickets alive.** Update the Linear issue when reality diverges from the plan: scope changes, things get descoped, blockers surface, acceptance criteria shift. The ticket should reflect what's actually happening, not what was originally imagined.

### Branches

Named `{github-username}/mon-{N}-{slug}`. One branch per Linear issue. Branch off `master`.

For commit conventions, research-plan / implementation-note placement, and the "keep docs alive" rule, see [CLAUDE.md](./CLAUDE.md).
