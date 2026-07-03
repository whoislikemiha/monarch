# Design log

This directory is the working design log for Monarch — a per-feature paper trail kept alongside the code. Each feature is tracked from research through to landing, so the *why* behind a change lives next to the *what*.

## Layout

- **`plan/`** — research plans written *before* implementation. Each one scopes a change: the problem, the relevant files, the approach, decisions locked in, and what's explicitly out of scope.
- **`impl/`** — implementation notes written *after* a change lands. What actually shipped, the key decisions (and where reality diverged from the plan), files touched, and what was deliberately left out.
- **`design/`** — longer-lived design docs that span multiple features. Notably `design/shadow-cognition/` (the agent memory / attention / distillation architecture and its phased roadmap) and `design/visual-direction/` (visual language exploration).
- **`spike/`** — occasional throwaway investigations that fed into a plan.

Most documents are named `MON-N` (e.g. `MON-124`). **`MON-N` is an internal issue id**, and any `linear.app/...` links point at a private issue tracker — they are internal references, not public URLs, and won't resolve for outside readers. The prose stands on its own without them.

## Worth reading first

- [`impl/MON-124.md`](./impl/MON-124.md) — turning a thin status strip into a real execution timeline; a good example of a mostly-frontend feature with a small backend seam.
- [`impl/MON-37.md`](./impl/MON-37.md) — an ordered persistence pipeline via a single-consumer channel; a focused backend/architecture change.
- [`design/shadow-cognition/README.md`](./design/shadow-cognition/README.md) — the entry point to the agent-cognition design work (memory, attention, distillation), the most ambitious thread in the log.

## Terminology note

Earlier notes use the project's former, themed vocabulary, which has since been standardized to industry-standard terms. When reading older documents, translate:

| Older term            | Standardized term   |
| --------------------- | ------------------- |
| shadow                | agent               |
| quest                 | objective           |
| keeper (memory keeper)| curator             |
| captain               | supervisor          |
| oath                  | persona / system prompt |

The product name **Monarch** is unchanged. The concepts these documents describe are current even where the wording predates the rename.
