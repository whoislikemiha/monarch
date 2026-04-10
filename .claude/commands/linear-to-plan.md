---
description: Turn a user request into a Linear issue, a Git branch, and a research-level plan
---

You are turning `$ARGUMENTS` into three artifacts: a Linear issue, a Git branch, and a research plan. Do not write code in this command — planning only.

**Ask clarifying questions at any point if the request is ambiguous, the scope is unclear, or you need to pick between reasonable alternatives. Do not guess on things that matter.**

## 1. Understand the request

Read `$ARGUMENTS` carefully. Before doing anything else:

- If the goal, scope, or success criteria are unclear, ask the user.
- If you can think of two plausible interpretations, ask the user which one they mean.
- If the work spans multiple unrelated concerns, ask whether to split it into multiple issues.

Do not proceed to step 2 until you are confident about what "done" looks like.

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

- **labels** — pick one `Area/*` label and one `Type/*` label that fit (e.g. `frontend` + `refactor`, `docs` + `polish`, `backend` + `spike`). If unsure which apply, ask the user.
- **assignee** — `me` unless the user says otherwise.
- **project** — if the work clearly belongs to an existing project (e.g. `Agent loop`, `Memory & context tools`), attach it. Otherwise leave blank and mention it to the user.

After creation, **capture the `gitBranchName` field from the response** — it looks like `markocvijanovic1998/mon-5-update-readme`. This is the exact branch name to use in step 3.

## 3. Create the Git branch

Run from `master` (or whichever base branch the user specifies). Use the exact `gitBranchName` value from Linear:

```bash
git fetch origin master
git checkout -B <gitBranchName> origin/master
git push -u origin <gitBranchName>
```

Do **not** make any commits on the branch in this command. Pushing an empty branch is fine — it sets up the tracking and makes the branch visible.

If the branch already exists remotely with unrelated commits, stop and ask the user how to proceed — do not force-push.

## 4. Research and produce a plan

Now investigate the codebase to understand what touching this would actually involve. Use `Grep`, `Read`, `Glob`, or spawn the `Explore` agent for broad searches. Read [ONBOARDING.md](../../ONBOARDING.md) if you need a map of the repo.

The output is a **research plan**, not an implementation. It must contain:

1. **Summary** — one paragraph restating the goal in your own words, grounded in what you found in the code.
2. **Relevant files and areas** — specific file paths with short descriptions of what lives there and why it's relevant. Include line number hints where useful.
3. **What needs to change** — at the module / concept level. Describe the shape of the change, not the code. Example: "Extend `AgentManager` in `src-tauri/src/agent.rs` with a new command path that persists X before routing to the sidecar" — **not** a code snippet.
4. **Open questions** — anything you're uncertain about, could be done multiple ways, or where you want the user's input before implementation starts.
5. **Out of scope reminders** — what this plan explicitly does not cover.

Do **not** include:
- Code snippets or pseudocode
- Line-by-line diffs
- Concrete implementation details beyond the conceptual level

## 5. Hand off

Report back to the user with:

- The Linear issue URL and identifier (e.g. `MON-7`).
- The branch name you created and pushed.
- The research plan from step 4.
- Any open questions from step 4 that block moving to implementation.

End by telling the user they can invoke `/plan-to-impl` when they are ready to start building, or reply inline with answers to open questions.
