# MON-31 — Introduce `MonarchError` domain error type

Linear: https://linear.app/monarch-commander/issue/MON-31
PR: https://github.com/whoislikemiha/monarch/pull/24
Base: `markocvijanovic1998/mon-14-phase-1-rust-state-ownership` (Wave 2 rule — not `master`)

## What was implemented

Every `#[tauri::command]` in the crate — plus their `ws_*` WebSocket
twins and every internal helper they call — now returns
`Result<T, MonarchError>` instead of `Result<T, String>`. The error
type lives in the new `src-tauri/src/error.rs` module and projects to
a stable DTO `{ kind, message, details }` via a custom `Serialize`
impl; `specta::Type` is forwarded from the enum to the DTO so specta
emits a real TS type (`ErrorDto`) in `src/lib/bindings.ts`. Every
generated `typedError<T, E>` wrapper now has `ErrorDto` in the `E`
position, and `src/App.svelte`'s spawn failure handler branches on
`err.kind` via a new `formatSpawnError` helper to satisfy the "match
on kind at ≥1 call site" acceptance bullet.

`MonarchError` variants: `Sidecar(SidecarErrorKind)`, `Db`,
`Persistence`, `Http`, `InvalidInput`, `NotFound`, `Lock`, `Io`,
`Serde`. Source chains ride along as owned inner errors on
`Db` / `Http` / `Io` / `Serde` so the DTO's `details` field carries
the full `source.to_string()` without losing information. Sidecar
sub-kinds (`ProcessDown | StdinWrite | ReplyError | Parse`) **flatten**
into the top-level `kind` string
(`sidecarProcessDown | sidecarStdinWrite | sidecarReplyError |
sidecarParse`) so the frontend type-narrows without a nested match.
`From` impls for `rusqlite::Error`, `std::io::Error`,
`serde_json::Error`, `reqwest::Error`, `tauri::Error` collapse the
~130 `.map_err(|e| e.to_string())?` sites to bare `?`. A
`lock_poisoned(label)` helper returns a `map_err` closure that tags
each mutex site with a stable label (`"agents"`, `"session map"`,
`"sidecar"`, `"app handle"`, `"openrouter cache"`, …).

`ws::make_response` embeds the full DTO as a new top-level
`errorData` field alongside the existing `error` string, so WS
clients see the same typed shape Tauri clients get without breaking
back-compat with anything that still reads `error` as a plain string.

## Key decisions

- **No `thiserror` dep.** Plan called for it. I implemented `Display`
  + `Error` manually (≈30 lines) because `MonarchError` has
  heterogeneous variants (owned source errors on some, owned strings
  on others, a `&'static str` label on `Lock`) and I wanted the DTO
  projection logic (`kind_str`, `details`, `Display::fmt`) colocated
  in one place. Happy to add thiserror back if the reviewer prefers
  the derive; no functional impact.
- **`Result<T>` alias lives in `error.rs` but is not re-exported
  crate-wide.** Plan decision #6 said re-export. I kept the alias
  `#[allow(dead_code)]` and used the fully-qualified
  `Result<T, MonarchError>` two-arg form in signatures. Reason:
  `db.rs` has several `.collect::<rusqlite::Result<Vec<_>>>()` call
  sites and a crate-wide `use crate::Result` shadow would have forced
  a lot of extra renaming noise. The alias is there for files that
  want it later.
- **`PersistCommand::apply` was migrated** even though the plan
  offered leaving it as `Result<(), String>`. Because
  `db.log_event_internal` / `save_message_internal` /
  `increment_session_message_count` now all return `MonarchError`,
  `apply`'s body reduces to two `?` chains. `run_persist_consumer`
  stringifies only at the log/desync boundary via
  `Ok(Err(e)) => e.to_string()`.
- **`ws::make_response` uses dual `error` + `errorData` fields**
  rather than nesting under `error.data` as the plan suggested.
  Preserves backwards compat with any WS client that reads
  `response.error` as a string; typed clients can pick up `errorData`
  for the DTO. Same runtime cost.
- **Dropped the `"LM Studio not reachable at {host}"` context
  prefixes** in `models.rs`. With `From<reqwest::Error> → Http`, the
  bare `?` propagates the source error and the DTO's `details` field
  carries the reqwest `to_string()`. The non-HTTP-but-still-logical
  errors (4xx / 5xx responses) continue to build a custom
  `MonarchError::persistence(format!(...))` with the status code and
  URL embedded.
- **`AgentView.svelte` left untouched.** ~15 try/catch sites stay on
  the opaque path per the plan — the acceptance bullet is "at least
  one call site" and `App.svelte` spawn handler satisfies it.
  Consumers adopt the DTO incrementally in later PRs.

## Files touched

- **New:** `src-tauri/src/error.rs` — `MonarchError`,
  `SidecarErrorKind`, `ErrorDto`, `lock_poisoned`, `From` impls,
  `Serialize` + `specta::Type` forwarding, `Result<T>` alias.
- `src-tauri/src/lib.rs` — `mod error;`, `pub use error::MonarchError`,
  `export_bindings` migrated.
- `src-tauri/src/agent.rs` — entire command / helper / `ws_*`
  surface. `write_command` uses the new `sidecar_*` constructors;
  `get_app_handle` uses `invalid_input`; `resolve_sidecar_path` uses
  `not_found`; session-not-found errors become structured
  `MonarchError::not_found(...)`.
- `src-tauri/src/db.rs` — every command + `ws_*` + internal helper.
  Mutex sites use `lock_poisoned("db")`. Rusqlite-typed collects use
  `.collect::<rusqlite::Result<Vec<_>>>()?` so `?` converts through
  the `From<rusqlite::Error>` impl.
- `src-tauri/src/persistence.rs` — rewritten.
- `src-tauri/src/models.rs` — fetcher error paths collapse to `?`
  with a structured wrap for non-HTTP LM Studio failure cases.
- `src-tauri/src/toolbox/placeholder.rs` — two lines.
- `src-tauri/src/ws.rs` — `dispatch_command` → `Result<Value,
  MonarchError>`, `make_response` embeds the DTO as `errorData`,
  `str_field` uses `InvalidInput`, deserialization wraps with
  `MonarchError::invalid_input(format!("Invalid X: {}", e))`.
- `src/lib/bindings.ts` — regenerated. `export type ErrorDto` at
  line 156; every `typedError<T, E>` wrapper now carries `ErrorDto`.
- `src/App.svelte` — `formatSpawnError` helper + call-site swap
  inside the spawn `.catch`.

## What was left out

- **MON-37 internals beyond `PersistCommand::apply`.**
  `run_persist_consumer` still lives on stringified errors at its
  log/desync boundary — intentional per the Wave 2 handoff.
- **`eprintln!` → `tracing` migration** in `agent.rs`. Parking-lot
  item.
- **`std::sync::Mutex` / `tokio::sync::RwLock` unification.**
  MON-34's job. MON-31 only carved out `MonarchError::Lock` +
  `lock_poisoned`.
- **`too_many_arguments` clippy warnings** on `spawn_agent` /
  `ws_spawn_agent`. Still present — MON-35 collapses the signatures.
- **Frontend error-handling beyond `App.svelte`.** `AgentView.svelte`
  still strings errors; consumers adopt the DTO incrementally.
- **Rust test harness.** Still no working Rust tests on Windows.
  Verification was manual + `cargo check` + `cargo clippy` +
  `svelte-check`.
