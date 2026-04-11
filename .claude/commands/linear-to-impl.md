---
description: Create a Linear issue, a Git branch, and jump straight to implementation (skip planning for straightforward changes)
---

You are turning `$ARGUMENTS` into a Linear issue, a Git branch, and shipped code — in one pass. Use this instead of `/linear-to-plan` when the change is small or obvious enough that a formal research plan would be overhead.

**Ask clarifying questions at any point if the request is ambiguous, the scope is unclear, or you need to pick between reasonable alternatives. Do not guess on things that matter.**

## 1. Understand the request

Read `$ARGUMENTS` carefully. Before doing anything else:

- If the goal, scope, or success criteria are unclear, ask the user.
- If you can think of two plausible interpretations, ask which one they mean.
- If the work spans multiple unrelated concerns, ask whether to split it.

Do not proceed until you are confident about what "done" looks like.

## 2. Create the Linear issue

Use `mcp__plugin_linear_linear__save_issue` with team `Monarch`. The description **must** follow this template verbatim (fill each section; omit none):

```markdown
## Description
Short paragraph of context — what this is, where it sits in the project, why it exists.

## Goal
One or two sentences describing the outcome. What does success look like?

## Acceptance criteria
- [ ] Concrete, checkable criterion
- [ ] Another concrete criterion
- [ ] ...

## Out of scope
- What this issue explicitly does not cover
```

Also set:

- **labels** — pick one `Area/*` label and one `Type/*` label that fit. If unsure, ask.
- **assignee** — `me` unless the user says otherwise.
- **project** — if the work clearly belongs to an existing project, attach it. Otherwise leave blank.

After creation, **capture the `gitBranchName` field from the response** — this is the exact branch name to use in step 3.

## 3. Create the Git branch

Run from `master` (or whichever base branch the user specifies). Use the exact `gitBranchName` value from Linear:

```bash
git fetch origin master
git checkout -B <gitBranchName> origin/master
git push -u origin <gitBranchName>
```

If the branch already exists remotely with unrelated commits, stop and ask the user — do not force-push.

## 4. Implement

No separate plan file. Read the relevant code, understand it, then implement. Guidelines:

- **Small, coherent commits.** One conceptual change per commit. Don't bundle unrelated edits.
- **Conventional commit messages** matching the repo style (see `git log`).
- **Follow project conventions** documented in [CLAUDE.md](../../CLAUDE.md) and [ONBOARDING.md](../../ONBOARDING.md).
- **Run checks locally** before each commit where it makes sense: `svelte-check`, `cargo check`, sidecar `tsc`, etc. Don't commit code that doesn't build.
- **Stop and ask** the moment you hit something unexpected or a decision the user should own.

Push your commits to the branch as you go.

## 5. Review checkpoint — stop and wait

Once you believe the work is done, **stop**. Do not open a PR yet. Report back with:

- A summary of what you changed, grouped by concern.
- Which acceptance criteria are now satisfied.
- Anything you deviated from or left as a TODO.
- Concrete things for the user to verify — commands to run, behaviors to check.

Then wait for the user's response.

## 6. Review loop

When the user comes back with feedback:

- **Approved / ship it** → go to step 7.
- **Small fixes** → make them, commit, push, return to checkpoint (step 5).
- **Bigger rework** → surface it explicitly, ask whether to update the Linear issue, then loop.
- **Ambiguous feedback** → ask clarifying questions before acting.

Repeat until the user approves.

## 7. Ship

Once approved:

1. Make sure all work is committed and pushed.
2. Open a PR with `gh pr create`, including the Linear issue identifier (e.g. `MON-7`) at the top of the body so the Linear GitHub integration links them.
3. Report the PR URL to the user.
4. Ask whether they want the Linear issue state updated — do not change state without asking.

Do not merge the PR. The user merges.

## 8. Write and commit implementation notes

After the PR is created, write a high-level summary to `thoughts/impl/<issue-id>.md`. Keep it short — this is a reference for future context, not documentation. Include:

- **What was implemented** — brief description in plain language.
- **Key decisions** — notable design choices, trade-offs, or deviations.
- **Files touched** — main files created or modified.
- **What was left out** — anything deferred or descoped.

**Commit and push immediately:**

```bash
git add thoughts/impl/<issue-id>.md
git commit -m "docs(<issue-id-lowercase>): implementation notes"
git push
```

Do not leave impl files uncommitted — they are part of the PR.
