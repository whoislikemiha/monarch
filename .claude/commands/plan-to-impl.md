---
description: Implement a plan with a human review checkpoint, looping until approved
---

You are taking a plan (from `$ARGUMENTS` or from the prior `/linear-to-plan` output in this conversation) and turning it into shipped code. This command has a **mandatory human review checkpoint** — you do not finish until the user explicitly approves.

**Ask clarifying questions at any point.** If the plan is ambiguous, if a decision has multiple reasonable answers, if your research surfaces something the plan didn't anticipate — stop and ask before committing to an approach. Asking is cheap; undoing is expensive.

## 1. Confirm the plan

Before you touch any code:

- Restate the plan in 3–5 bullet points so the user can sanity-check your understanding.
- List anything in the plan that is ambiguous, underspecified, or that you'd like to deviate from — and ask.
- If the plan references files you haven't read yet, read them now so your understanding is grounded in the current code, not assumptions.
- Verify you are on the correct branch (the one from `/linear-to-plan`, or whatever the user specifies). If not, `git checkout` to it.

Only proceed to step 2 once the user has confirmed your understanding (or it's obvious enough that confirmation isn't needed — use judgment, and err toward asking).

## 2. Implement

Work through the plan in logical chunks. Guidelines:

- **Small, coherent commits.** One conceptual change per commit. Don't bundle unrelated edits.
- **Conventional commit messages** matching the repo style (see `git log`).
- **Follow project conventions** documented in [CLAUDE.md](../../CLAUDE.md) and [ONBOARDING.md](../../ONBOARDING.md) — especially the "Rust owns persistence" rule, the session ancestry model, and the sidecar protocol.
- **Run checks locally** before each commit where it makes sense: `svelte-check`, `cargo check`, sidecar `tsc`, etc. Don't commit code that doesn't build.
- **Narrate key decisions briefly** — when you hit a fork in the road, one sentence explaining the direction you chose and why. Longer only if it matters.
- **Stop and ask** the moment you realize the plan is wrong, the code reveals something unexpected, or you're about to make a decision the user should own.

Push your commits to the branch as you go so the user can follow along on GitHub.

## 3. Review checkpoint — stop and wait

Once you believe the plan is implemented, **stop**. Do not open a PR yet. Do not mark anything complete. Report back to the user with:

- A summary of what you changed, grouped by concern.
- Which parts of the acceptance criteria are now satisfied (reference them explicitly).
- Anything you deviated from or decided along the way that the user should know about.
- Anything you skipped, deferred, or left as a TODO (with a reason).
- Concrete things for the user to verify — commands to run, behaviors to check, files to eyeball.

Then say you are waiting for review and **wait for the user's response.** Do not proactively do more work during this checkpoint.

## 4. Review loop

When the user comes back with feedback, classify it:

- **Approved / ship it** → go to step 5.
- **Small fixes** → make them, commit, push, and return to the checkpoint (step 3).
- **Bigger rework** → if the feedback changes the plan meaningfully, surface that explicitly. Ask whether to update the Linear issue's acceptance criteria, then loop back through step 2 with the revised understanding.
- **Ambiguous feedback** → ask clarifying questions before acting.

Repeat step 3 → step 4 until the user approves. There is no limit on iterations; quality over speed.

## 5. Ship

Once the user approves:

1. Make sure all work is committed and pushed.
2. Open a PR with `gh pr create`, including the Linear issue identifier (e.g. `MON-7`) at the top of the body so the Linear GitHub integration links them automatically.
3. Report the PR URL to the user.
4. Ask whether they want the Linear issue state updated (e.g. moved to `In Review`) — do not change state without asking.

Do not merge the PR. The user merges.

## 6. Write implementation notes

After the PR is created, write a high-level implementation summary to `thoughts/impl/<issue-id>.md` (e.g. `thoughts/impl/MON-7.md`). Create the `thoughts/impl/` directory if it doesn't exist.

The file should be a concise, high-level record of what was done — not a changelog or diff. Include:

- **What was implemented** — a brief description of the feature/fix/change in plain language.
- **Key decisions** — any notable design choices, trade-offs, or deviations from the original plan.
- **Files touched** — list the main files that were created or modified (no need to be exhaustive with trivial edits).
- **What was left out** — anything deferred, descoped, or intentionally skipped.

Keep it short — this is a reference for future context, not documentation.
