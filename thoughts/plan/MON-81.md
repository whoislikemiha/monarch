# MON-81 — Fetch Anthropic and OpenAI Codex models dynamically

## Summary

`get_models` in `src-tauri/src/models.rs` returns hardcoded catalogues for `anthropic` and `openai-codex` while `openrouter` and `lmstudio` already fetch live and cache for an hour through `ModelCache`. As a result Anthropic shows a stale 4.6-era list (no Claude Opus 4.7 / Sonnet 4.6) and Codex is locked to a single `gpt-5.4` entry — reinforced by `fixedModelId` in `src/lib/ModelSelector.svelte:46` that turns the Codex model input into a read-only field. The fix is to treat both providers the same way OpenRouter is already treated: hit each vendor's `/v1/models` endpoint with the OAuth credential Pi already stores in `~/.pi/agent/auth.json`, cache the result for an hour, drop the hardcodes, and let the existing Retry button refresh on demand. Spawn flow, sidecar protocol, and session lifecycle are untouched — this is a self-contained change to the discovery path inside `models.rs` plus three small frontend tweaks (`fixedModelId` removal, `REFRESHABLE_PROVIDERS` extension, no Codex hint copy).

## Relevant files and areas

- `src-tauri/src/models.rs` — entire surface for this change.
  - `ModelCache` struct (lines 32-44) — currently holds only the OpenRouter slot; needs two more `Mutex<Option<(Vec<ModelInfo>, Instant)>>` fields and a shared cache-lookup/store helper to avoid repeating the OpenRouter pattern three times.
  - `pi_auth_path` / `pi_auth_entry_exists` (lines 46-67) — already locate `~/.pi/agent/auth.json` and parse it as JSON; need a sibling helper that returns the OAuth `access` token (and `expires` if useful) for a given provider, not just an exists check.
  - `anthropic_models` / `openai_codex_models` (lines 70-97) — to be deleted.
  - `fetch_openrouter_models` (lines 234-256) — the canonical shape new fetchers should mirror (reqwest client with timeout, deserialize, map to `ModelInfo`).
  - `get_models` (lines 258-292) and `ws_get_models` (lines 296-319) — the two parallel match arms that gate provider → cache/fetch. Both arms currently duplicate the cache check + refresh logic; consolidating into a single `cached_or_fetch(&ModelCache, provider, fetcher)` helper before adding two more providers will keep the diff small.
- `src-tauri/src/error.rs` — `MonarchError` already covers `reqwest::Error`, `serde_json::Error`, and a `persistence` constructor; the new fetchers can reuse those without a new variant. Worth a glance to confirm we surface a useful message when the OAuth token is missing or expired (e.g. `MonarchError::persistence("Anthropic credentials not found in ~/.pi/agent/auth.json")`).
- `src-tauri/src/lib.rs:176` and `ws.rs:267` — wiring is already in place; no change needed beyond what's already exported.
- `src/lib/ModelSelector.svelte`
  - Line 46: `fixedModelId = $derived(provider === "openai-codex" ? "gpt-5.4" : "")` — to be deleted.
  - Lines 98, 145-146, 242-256, 261, 270, 287-291 — every other branch on `fixedModelId` (`readonly`, the `Uses your Pi Codex login` placeholder, the keydown short-circuit, the `field-hint` block) collapses once the derived is gone. Most of those become dead code; a few (the placeholder fallbacks for loading/error/empty states) are already correct without `fixedModelId` and just need the outer `fixedModelId ? ... :` shell removed.
- `src/lib/providers.ts:19-22` — `REFRESHABLE_PROVIDERS` set; add `anthropic` and `openai-codex` so the ↻ button renders for them too.
- `~/.pi/agent/auth.json` (read-only reference, not edited) — the credential source.
  - `openai-codex`: `{ type: "oauth", access: "<JWT>", refresh, expires, accountId }`. The JWT's `aud` claim is `https://api.openai.com/v1`, so the `access` token can be sent as `Authorization: Bearer <access>` against `https://api.openai.com/v1/models`.
  - `anthropic`: `{ type: "oauth", access: "sk-ant-oat01-...", refresh: "sk-ant-ort01-...", expires }`. The `access` token is what Pi sends to `api.anthropic.com`. The exact header convention (`x-api-key` vs `Authorization: Bearer` plus a beta header) is the one open question — see below.
- Reference for response shapes:
  - Anthropic `GET https://api.anthropic.com/v1/models` returns `{ data: [{ id, display_name, type: "model", created_at, max_input_tokens, max_tokens, capabilities }], has_more, first_id, last_id }`. We only need `id` + `display_name`.
  - OpenAI `GET https://api.openai.com/v1/models` returns `{ object: "list", data: [{ id, object, created, owned_by }] }`. Only `id` is meaningful for parity with today.
- `thoughts/impl/MON-79.md` — context for why this exists. MON-79 review surfaced the stale 4.6 list and the `fixedModelId` lock-in; this is the spun-out follow-up.

## What needs to change

1. **Extend `ModelCache` to hold three slots, not one.** Add `anthropic` and `openai_codex` `Mutex<Option<(Vec<ModelInfo>, Instant)>>` fields alongside the existing `openrouter` slot. Initialise in `ModelCache::new()`. The 1-hour `CACHE_TTL` constant stays as-is and is shared.
2. **Introduce a single cache-or-fetch helper.** Today `get_models` and `ws_get_models` each open-code the lock → check TTL → fetch → re-lock → store sequence for OpenRouter. Before adding two more providers, factor that pattern into one helper that takes the cache slot reference and an async fetcher closure. This collapses three near-identical match arms into one and keeps the diff per provider trivially small. Without this step the file grows considerably and the next provider-fetch ticket pays the same cost again.
3. **Add an OAuth-token reader for Pi auth.** Sibling to `pi_auth_entry_exists`, return `Option<String>` (or a small struct with `access` + `expires`) for a given provider key. Reuse the same `pi_auth_path` + `serde_json::Value` walk; the existing `exists` check can be re-expressed as `.is_some()` on the new reader.
4. **Add `fetch_anthropic_models`.** Mirror `fetch_openrouter_models` shape:
   - Read the OAuth access token via the new helper. If absent, fall back to `std::env::var("ANTHROPIC_API_KEY")`. If both are absent, return `MonarchError::persistence` with a message that names both sources so the surfaced `modelsError` in `ModelSelector` is actionable.
   - Build a `reqwest::Client` with a sane timeout (10s — match OpenRouter, since Anthropic responses are similar in size).
   - `GET https://api.anthropic.com/v1/models` with `anthropic-version: 2023-06-01` plus the auth header. Auth header convention is the one open question — see below.
   - Deserialize into a small private struct (`{ data: Vec<{ id, display_name }> }`), map to `ModelInfo { id, name: display_name, provider: "anthropic", context_window: None }`. Issue says id + name parity is enough; capability metadata, context window, and the `created_at` sort hint are explicitly out of scope.
5. **Add `fetch_openai_codex_models`.** Same shape, against `https://api.openai.com/v1/models` with `Authorization: Bearer <access token>`. The OpenAI list returns dozens of unrelated SKUs (embeddings, whisper, tts, dall-e, image-1, …) — see open question on whether to filter. Map to `ModelInfo { id, name: id, provider: "openai-codex", context_window: None }`.
6. **Delete `anthropic_models` and `openai_codex_models`** and their match arms in both `get_models` and `ws_get_models`. Replace each with a call to the new cached-or-fetch helper.
7. **Frontend cleanup** (no behaviour change beyond what's specified):
   - Remove `fixedModelId` derived and every conditional branch that guards on it. The Codex model input becomes an ordinary searchable picker with the same placeholder/error/empty-state copy the other providers already use.
   - Drop the `Uses Pi's existing 'openai-codex' auth and locks this provider to GPT-5.4` field-hint block — the `auth-status` block above already states the auth source.
   - Add `"anthropic"` and `"openai-codex"` to `REFRESHABLE_PROVIDERS` in `providers.ts` so the ↻ refresh button renders. The button itself already calls `fetchModels(provider)`, which will hit the new cache-busting path on the Rust side via the explicit re-fetch (cache TTL doesn't matter when the call comes through — but see open question 4 on whether the Retry button should explicitly bust the cache).
8. **Bindings regen.** Frontend types stay identical (`ModelInfo`, `ProviderAuthStatus`, the `get_models` signature) — no `cargo run -- --export-bindings` regen is required unless an unrelated structural change creeps in. Worth a `cargo check` and `npx svelte-check` pass after the change to confirm.

## Open questions

1. **Anthropic auth header for OAuth tokens.** Pi stores `sk-ant-oat01-...` (OAuth access token) for the `anthropic` provider. The public docs example uses `x-api-key: $ANTHROPIC_API_KEY` with regular `sk-ant-api03-...` keys; OAuth tokens typically go as `Authorization: Bearer <token>` and may require an additional `anthropic-beta` header. Need a quick spike at implementation time:
   - Try `x-api-key: <oat>` first (since some OAuth flows accept the access token in the same header).
   - Fall back to `Authorization: Bearer <oat>` plus the relevant beta header (the Claude Code / Pi convention).
   The fallback for `ANTHROPIC_API_KEY` (a real `sk-ant-api03-...` key) is unambiguously `x-api-key`, so the helper needs to choose the header based on which credential it found, not blindly apply one.
2. **Codex auth fallback env var.** The ticket calls out `ANTHROPIC_API_KEY` as the Anthropic env fallback but is silent on Codex. Today's `get_provider_auth_status` for `openai-codex` says "No Pi Codex auth found. Run Pi login for OpenAI Codex first." with no env-var alternative. Should the new fetcher accept `OPENAI_API_KEY` as a fallback (consistency with Anthropic), or stay Pi-only (consistency with the existing auth-status copy)? Default: Pi-only — but ask before implementing.
3. **Codex models filtering.** `https://api.openai.com/v1/models` returns the entire OpenAI catalogue (`gpt-4o`, `gpt-4o-mini`, `o1`, `o3-mini`, `gpt-5`, `gpt-5.4`, `text-embedding-3-large`, `whisper-1`, `dall-e-3`, `tts-1`, `omni-moderation-latest`, …). Showing all of them in a chat-model picker would be noisy and misleading (you can't have a conversation with `text-embedding-3-large`). Options, in order of escalating scope:
   - (a) Surface raw list — matches the literal "id + name parity" wording, but UX regression.
   - (b) Filter to a hand-maintained set of id prefixes (`gpt-`, `o1`, `o3`, `o4`, `codex-`) — keeps the picker chat-only without claiming to be a real capability filter.
   - (c) Out of scope — defer until OpenAI adds a usable `capabilities` field.
   Recommend (b); ask before going wider.
4. **Retry button cache-bust semantics.** Today's Retry just calls `fetchModels(provider)`, which hits Rust's `get_models`, which always returns the cached value if TTL hasn't expired — so on OpenRouter / LM Studio the Retry button is effectively a no-op for 60 minutes after a successful fetch. The ticket says the Retry button should "bust the cache as it does for OpenRouter / LM Studio" — but it doesn't actually do that today (LM Studio is uncached, OpenRouter is cached). Two options:
   - (a) Match today's behaviour exactly: cache hit wins, Retry is a no-op until TTL. Cheap, consistent with existing OpenRouter UX, but the ticket reads aspirationally otherwise.
   - (b) Add an explicit `force_refresh: bool` query arg to `get_models` and have the Retry button pass `true`. Bigger change (touches `bindings.ts` and the WS dispatch in `ws.rs`) but matches the ticket's stated intent and is genuinely useful when a provider transiently 5xx's.
   Recommend (a) for this PR (matches the literal "same as OpenRouter" reading), spin (b) out separately if we want it.
5. **Token-refresh failure mode.** Pi's OAuth tokens expire (`expires` is a millis timestamp; the Codex JWT in the local file expires in ~10 days, the Anthropic access token in ~1 day). Pi's sidecar refreshes them on the next session start, but a Rust-side fetch that runs while the token is stale will get a 401. Acceptable resolution: surface the 401 as `modelsError`, let the user spawn a quick session (which causes Pi to refresh), then hit Retry. Implementing OAuth refresh in Rust is squarely out of scope for this ticket. Confirm.
6. **Display name source for Anthropic.** Use `display_name` from the API response (`"Claude Opus 4.7"`) for `ModelInfo.name`, falling back to `id` if absent. Trivial but worth flagging since today's hardcoded names are similar but not byte-identical to what the API returns.

## Out of scope

- Spawn / session-lifecycle / sidecar-protocol changes. This ticket is discovery only.
- Reworking `ModelCache` into a generic per-provider map. Two extra named fields are fine.
- Pricing, context-window, capability, or thinking-mode metadata on Anthropic / OpenAI entries. Pure id + display name parity with today.
- Implementing OAuth token refresh in Rust. Stale-token failures surface as `modelsError`.
- Changing the auth-status copy strings in `get_provider_auth_status` (unless open question 2 resolves toward adding `OPENAI_API_KEY` as a Codex fallback, in which case the Codex message gets an env-var mention).
- Re-ordering or curating the live model lists beyond whatever filter falls out of open question 3.
- The MON-79 Extract-gating work (already shipped) and any further changes to `SpawnForm`'s `canSpawn` logic.
