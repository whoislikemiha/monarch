# Static Testing Strategy

> Scope: **static** checks only — anything that runs without booting Tauri, the
> sidecar, or SQLite. Runtime/integration suites (Vitest unit tests, `cargo
> test`, future e2e) are mentioned only where they overlap with static gates.
> Owner: TBD. Status: proposal.

---

## 1. Why a strategy

Monarch is three stacks glued by two contracts:

| Stack | Language | Contract it owns | Contract it consumes |
|---|---|---|---|
| Frontend | Svelte 5 + TS | DOM / IPC calls | `bindings.ts`, event channel names |
| Sidecar | Node + TS | JSONL events to Rust | `sidecar_protocol.rs` shape |
| Backend | Rust (Tauri) | Tauri commands, DB schema, sidecar JSONL | — |

Drift between these contracts is our highest-leverage bug class — and it is
entirely catchable statically. A second class — Svelte 5 runes used wrong,
Rust API surface accidentally widened, SQLite migrations missing — is also
fully catchable before the app boots.

The strategy below is therefore biased toward **contract and drift checks**
first, then ordinary correctness/style hygiene.

## 2. Taxonomy of static checks

We split static checks into five buckets. Each bucket gets a CI gate, a local
command, and an enforcement level (`error` / `warn` / `advisory`).

| Bucket | What it catches |
|---|---|
| **A. Type & compile** | TypeScript errors, Svelte 5 rune misuse, Rust compile errors |
| **B. Lint & style** | Footguns (unused vars, missing await, dead code), formatting |
| **C. Contract drift** | `bindings.ts` out of date, sidecar protocol mismatch, schema migrations missing |
| **D. Dependency hygiene** | Unused deps, vulnerable deps, license issues, lockfile drift |
| **E. Docs & repo invariants** | AGENTS.md key-file list current, no edits to `bindings.ts`, no secrets, no banned imports |

The buckets are independent — a failure in D should not mask a failure in A.

## 3. Per-layer recommendations

### 3.1 Rust (`src-tauri/`)

Current state: `cargo check`, `cargo test --lib` in CI. No clippy, no
rustfmt config, no `cargo deny`. 45 unit tests exist. Zero `unsafe`.

**Add:**

| Tool | Bucket | Level | Notes |
|---|---|---|---|
| `cargo fmt --check` | B | error | Add `rustfmt.toml` with `edition = "2021"`, default style. Zero-config is fine. |
| `cargo clippy --all-targets -- -D warnings` | B | error | Start with default lints. Curate `clippy.toml` only when noise demands it. Allow `clippy::too_many_arguments` repo-wide (Specta 10-arg limit forces request structs anyway). |
| `cargo check --all-targets --all-features` | A | error | Catches test-only code rotting. |
| `cargo deny check` | D | error → advisory first | Config: `advisories`, `licenses` (deny GPL-3.0+ in shipped binary), `bans` (no duplicate crate versions beyond an allowlist), `sources`. |
| `cargo machete` | D | warn | Unused dependencies in `Cargo.toml`. |
| `cargo doc --no-deps --workspace` w/ `RUSTDOCFLAGS="-D warnings"` | A | warn | Catches broken intra-doc links in module docstrings. |
| Bindings drift guard (custom) | C | error | See §4.1. |

**Do not add:** `cargo audit` separately — `cargo deny`'s advisories database
subsumes it. `miri` — overkill, no `unsafe`.

### 3.2 Sidecar (`sidecar/`)

Current state: `tsc` runs as part of `npm run build`. No linter. Strict mode
on. One test file (`tests/narration-tools.test.ts`).

**Add:**

| Tool | Bucket | Level | Notes |
|---|---|---|---|
| `tsc --noEmit` as standalone script | A | error | Split from build so CI can run check without producing `dist/`. |
| ESLint (flat config, `@typescript-eslint`) | B | error | Rules: `no-floating-promises`, `await-thenable`, `no-misused-promises`, `consistent-type-imports`, `no-explicit-any` (warn), `no-unused-vars` (error). |
| Prettier | B | error | Shared root config; sidecar inherits. |
| `knip` | D | warn | Unused files, exports, deps. Sidecar's small surface makes this cheap. |
| Protocol-shape check (custom) | C | error | See §4.2. |

**Strictness bump (deferred, advisory):** turn on `noUncheckedIndexedAccess`
and `exactOptionalPropertyTypes` in a follow-up ticket — both will surface
real bugs in JSONL parsing but require non-trivial cleanup.

### 3.3 Frontend (`src/`)

Current state: `svelte-check` available, Vitest configured, no lint, no
formatter. Aliased `$lib`. Svelte 5 runes only (per AGENTS.md).

**Add:**

| Tool | Bucket | Level | Notes |
|---|---|---|---|
| `svelte-check --tsconfig ./tsconfig.json --threshold error` | A | error | Already works; just wire to CI. |
| ESLint w/ `eslint-plugin-svelte` + `@typescript-eslint` | B | error | Rules above, plus `svelte/no-reactive-reassign`, `svelte/valid-compile`. |
| Custom rule / grep: ban `$:` reactive blocks and `writable(`/`get(` from `svelte/store` | C | error | Enforces "runes only" from AGENTS.md. Implement as ESLint `no-restricted-syntax` + `no-restricted-imports`. |
| Custom rule: ban direct `@tauri-apps/api` imports outside `src/lib/api.ts` | C | error | Enforces the IPC-abstraction rule. ESLint `no-restricted-imports` with allow-list. |
| Prettier (with `prettier-plugin-svelte`) | B | error | |
| `knip` | D | warn | High-value: Svelte components rot quickly. |
| `bindings.ts` immutability check | C | error | See §4.1. |

**Test layer (mentioned for completeness, not static):** Vitest unit tests
on stores run today; AgentRoster/AgentView and similar components are
implicitly covered only by humans. Out of scope for this doc, captured in
§6.

## 4. Cross-cutting contract checks

These are the highest-value gates. They are scripts, not off-the-shelf
linters.

### 4.1 `bindings.ts` drift

**Risk:** Rust command signatures change, dev forgets to regenerate, frontend
keeps compiling against stale types until a runtime IPC mismatch.

**Check:** In CI, after `cargo` build:
```bash
cd src-tauri && cargo run -- --export-bindings
git diff --exit-code src/lib/bindings.ts
```
Fails the build if generated output differs from committed file.

**Local:** `npm run check:bindings` script wrapping the same two commands.

### 4.2 Sidecar protocol shape

**Risk:** Rust `sidecar_protocol.rs` enum variants drift from sidecar's
`protocol.ts` event union; JSONL silently breaks at runtime.

**Options, in order of cost:**

1. **Cheap (start here):** a small `scripts/check-protocol-parity.mjs` that
   regex-extracts variant names from both files and asserts set equality.
   Catches added/renamed variants. Doesn't validate shape.
2. **Medium:** generate the TS protocol types from Rust via Specta (same
   mechanism as `bindings.ts`). Requires lifting protocol types to
   `#[derive(Type)]` and wiring an export. Highest payoff but a project
   in itself — file as a follow-up ticket.
3. **Heavy:** JSON-Schema as the source of truth, codegen both sides.
   Overkill given how few variants there are.

Recommendation: ship (1) now, file ticket for (2).

### 4.3 SQLite migration completeness

**Risk:** Column added to `CREATE TABLE` in `db/schema.rs` without a matching
idempotent `ALTER TABLE` block — fresh DBs work, existing ones crash.

**Check:** A `cargo test` (counts as static-ish — runs without Tauri) that:
1. Materializes the pre-launch schema from a snapshot string.
2. Runs `init_schema`.
3. Asserts the resulting `PRAGMA table_info` matches `init_schema` applied to
   an empty DB.

Already partly covered by existing tests; verify and extend.

### 4.4 Repo invariants

A single `scripts/check-invariants.mjs` runs cheap repo-wide greps:

- `src/lib/bindings.ts` was not edited by hand (git diff against last
  generation marker, or simpler: re-run §4.1 and trust it).
- No `console.log` in `src/` outside `src/lib/api.ts` debug paths
  (advisory warn).
- No `dbg!` / `println!` in `src-tauri/src/` outside `main.rs` (warn).
- No `TODO(secret)` / hard-coded API keys (regex: `sk-[A-Za-z0-9]{20,}`,
  `ghp_…`, etc.) — error.
- `AGENTS.md` "Start Here" table file paths all exist (catches link rot when
  files move) — error.

### 4.5 Lockfile drift

- `package-lock.json`, `sidecar/package-lock.json`, `Cargo.lock` must be
  committed and unchanged after `npm ci` / `cargo build` in CI. CI uses
  `npm ci` (already does for `cargo` via cache); add the assertion.

## 5. CI wiring

Replace today's single `rust-test.yml` with one workflow per concern, so a
failure is named usefully in the PR check list.

```
.github/workflows/
  rust.yml          # fmt, clippy, check, test, deny, machete, bindings-drift
  sidecar.yml       # tsc, eslint, prettier, knip
  frontend.yml      # svelte-check, eslint, prettier, knip, bindings-immutable
  invariants.yml    # scripts/check-invariants.mjs, scripts/check-protocol-parity.mjs
```

Each runs on `push` to `master` and on `pull_request`. Parallelism over
serialization — devs see all failures at once.

**Caching:** keep `Swatinem/rust-cache`; add `actions/setup-node` with
`cache: 'npm'` and `cache-dependency-path` listing both lockfiles.

**Required for merge (branch protection):** `rust`, `sidecar`, `frontend`,
`invariants` — all four. Advisory jobs (`cargo deny` until tuned, `knip`)
run under a separate `quality` workflow that is *not* required, so noise
doesn't block PRs while we calibrate.

## 6. Local developer story

A single root `npm run check` runs everything, in dependency order:

```jsonc
"scripts": {
  "check:rust":        "cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo check --all-targets",
  "check:bindings":    "cd src-tauri && cargo run --quiet -- --export-bindings && cd .. && git diff --exit-code src/lib/bindings.ts",
  "check:sidecar":     "npm --prefix sidecar run typecheck && npm --prefix sidecar run lint",
  "check:frontend":    "svelte-check && eslint src",
  "check:invariants":  "node scripts/check-invariants.mjs && node scripts/check-protocol-parity.mjs",
  "check":             "npm run check:rust && npm run check:bindings && npm run check:sidecar && npm run check:frontend && npm run check:invariants"
}
```

**Pre-commit (optional, opt-in):** a `lefthook.yml` (or `.husky/`) running
`prettier --write` + `eslint --fix` on staged files only. Heavy checks
(`cargo clippy`, `svelte-check`) stay out of pre-commit — they belong in
pre-push or CI. Recommend lefthook over husky: single static binary,
zero-npm dependency on the hook itself.

## 7. Phased adoption

The current repo has ~45k lines and zero linting beyond `tsc`. Turning every
gate to error on day one will produce a wall of red and stall. Phase it:

**Phase 1 — Format + drift (1 PR, ~half day).** Pure mechanical.
- `rustfmt.toml`, `cargo fmt` over the tree, `cargo fmt --check` in CI.
- Prettier root config + Svelte plugin, format the tree.
- Bindings-drift CI gate (§4.1).
- Invariants script with just: bindings immutability, key-file existence,
  secret regex. Wire to CI as required.

Phase 1 alone closes the highest-frequency real bugs (stale bindings) and
ends the format bikeshed.

**Phase 2 — Lint at warn (1 PR).** Add clippy, ESLint, `svelte-check` to CI
as **warn** (continue-on-error). Surface the noise as a one-time issue,
triage it into:
- "fix in this phase" — the small wins.
- "fix in dedicated tickets" — the genuine refactors.
- "allow rule-wide" — when noise > value, codify in config.

**Phase 3 — Lint at error (1 PR).** Flip clippy/ESLint to error. By now the
backlog is small and the rule set is curated.

**Phase 4 — Contract & hygiene depth.** `knip`, `cargo deny`, `cargo
machete`, protocol-parity script. Land each as its own small PR so the
inevitable false-positive triage is bounded.

**Phase 5 — Strictness creep.** Sidecar `noUncheckedIndexedAccess`,
frontend rule banning `$:` and writable stores, protocol type
codegen via Specta. These each warrant their own ticket.

## 8. Out of scope (deliberately)

- **Unit & integration test strategy** — separate doc. Mentioned here only
  because §4.3 (migration check) lives in `cargo test`.
- **E2E / Tauri-driver tests** — separate doc.
- **Performance / load testing.**
- **Visual regression for Svelte components** — defer until UI stabilizes.
- **Mutation testing.** Premature.
- **`miri` / formal verification.** No `unsafe`, no need.

## 9. Open questions

1. **Are we OK with `prettier` reformatting the entire tree in one commit?**
   Alternative: `.git-blame-ignore-revs`. Recommend yes + ignore-revs.
2. **`cargo deny` license policy:** which licenses are blocked? Need a list.
   Default proposal: allow MIT, Apache-2.0, BSD-*, ISC, MPL-2.0; deny
   GPL-*, AGPL-*, SSPL.
3. **Do we want bindings drift on `push` only, or also as a
   `prepare-commit-msg` hook locally?** Latter is more nagging but catches
   issues earlier.
4. **Where does the protocol-parity script live long-term** — keep as
   regex, or invest in Specta-driven codegen? Recommend the latter once
   Phase 4 lands.

## 10. Concrete next actions

If approved, the first PR is mechanical and small:

1. Add `rustfmt.toml` (1 line). Run `cargo fmt`. Commit.
2. Add root `.prettierrc`, `.prettierignore`, install `prettier` +
   `prettier-plugin-svelte` as devDep. Run `prettier --write`. Commit on a
   separate "format only" commit so `.git-blame-ignore-revs` can list it.
3. Add `scripts/check-bindings.mjs` (4 lines wrapping the cargo command +
   `git diff --exit-code`).
4. Add `scripts/check-invariants.mjs` with the three Phase-1 checks.
5. Update `.github/workflows/`: rename `rust-test.yml` → `rust.yml`, add
   `fmt --check` step, add `invariants.yml`.
6. Update `AGENTS.md` "Build & Dev" with `npm run check`.

Estimated effort: half a day, no behavior change.
