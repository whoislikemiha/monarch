# MON-98 — Captain/Shadow Identity (P1)

## What was implemented

Captain identity (L1a) and shadow identity (L1b) end-to-end: two versioned identity blobs that get injected into every shadow's system prompt. Captain is a singleton global; shadow is per-agent. Both can be edited live via a new **Identity** toolbox tool, and updates are pushed to all active sessions immediately via `SetCustomPrompt` — no restart required.

## Key decisions

- **Row-per-version, no hard delete** — `captain_identity_versions` and `shadow_identity_versions` are append-only with a `supersedes_id` chain. Active version is `captain.current_version` / `agents.identity_version_id`. Rollback = pointer flip. Chosen to keep history without added complexity.
- **No circular FK** — `captain.current_version` is an unguarded integer (not a FK) to sidestep the bootstrap chicken-and-egg. Integrity enforced by `ensure_captain_bootstrap()` in application code.
- **Identity commands in `agent/commands.rs`** — the upsert commands need both `Database` and `AgentManager` (to push live refresh). Putting them in `db.rs` would create a circular import; `commands.rs` has both in scope.
- **`undefined` vs `string` in `setCustomPrompt`** — `undefined` means "leave stored value alone"; any string (even `""`) means "update". This lets captain-only or shadow-only refreshes leave the other side untouched.
- **Token heuristic** — `chars ÷ 4` with warn at 2400 est. tokens, hard save-block at 3000. Combined (captain + shadow) budget, since both land in the same system prompt.
- **Empty prompt injection** — blank identity payloads are silently omitted from the system prompt (no empty `## Captain` section rendered).

## Files touched

**Rust:**
- `src-tauri/src/db.rs` — schema tables (`captain`, `captain_identity_versions`, `shadow_identity_versions`, `ALTER TABLE agents`), `ensure_captain_bootstrap`, all read/upsert DB internals
- `src-tauri/src/sidecar_protocol.rs` — `captainIdentityPayload` / `shadowIdentityPayload` on `CreateSession` + `SetCustomPrompt`
- `src-tauri/src/agent/manager.rs` — fetch payloads at spawn, `refresh_captain_identity`, `refresh_shadow_identity`
- `src-tauri/src/agent/commands.rs` — 4 new Tauri commands + request structs
- `src-tauri/src/agent/mod.rs` — re-exports for new request structs
- `src-tauri/src/lib.rs` — command registration
- `src-tauri/src/ws.rs` — WebSocket dispatch arms

**Sidecar:**
- `sidecar/src/protocol.ts` — protocol type additions
- `sidecar/src/shadow-oath.ts` — `buildSystemPrompt` extended with captain/shadow sections
- `sidecar/src/runtime-manager.ts` — `ManagedSession` fields, `createSession` wiring, `setCustomPrompt` conditional update logic
- `sidecar/src/index.ts` — dispatch updated for new `set_custom_prompt` fields

**Frontend:**
- `src/lib/toolbox/tools/IdentityTool.svelte` — new toolbox tool (captain + shadow editors, token budget bar)
- `src/lib/toolbox/registry.ts` — registered at `order: 5`

## What was left out

- **Bindings regeneration** — `cargo run -- --export-bindings` not run (Rust not installed on this machine). The 4 new commands will appear in `bindings.ts` once Rust is installed and bindings are regenerated.
- **Rollback UI** — version history exists in DB but no UI to browse/revert. Deferred.
- **Image blobs** — identity payload is text-only for now. Image/avatar injection deferred.
- **Hard cap enforcement** — the UI warns and blocks the save button at 3000 est. tokens, but there is no server-side validation.
