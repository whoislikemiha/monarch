# MON-81 — Fetch Anthropic and OpenAI Codex models dynamically

## What was implemented

`get_models("anthropic")` and `get_models("openai-codex")` now resolve through the same `cached_or_fetch` helper that powers OpenRouter: a 1-hour in-memory TTL plus a `forceRefresh` arg the UI's Retry button can pass to bust the cache on demand. Live fetches against `https://api.anthropic.com/v1/models` (via `x-api-key`) and `https://api.openai.com/v1/models` (via Bearer) only happen when the corresponding env API key is set; without it the fetcher returns a curated in-Rust list of subscription-supported IDs. Pi's stored OAuth tokens cannot call `/v1/models` for either provider — Anthropic outright rejects OAuth on the listing endpoint, and ChatGPT JWTs lack the `api.model.read` scope — so the Pi-OAuth fetch path that the original plan envisioned was dropped during impl after live probing returned 401/403.

Frontend-side, the `fixedModelId` lock that pinned Codex to `gpt-5.4` is gone, the Retry button renders for Anthropic + Codex via an expanded `REFRESHABLE_PROVIDERS` set, and each model row is tagged `SUB` (green) or `API` (amber) when the provider has a meaningful subscription distinction. The auth-status header gets a colored chip naming the active mode (`subscription` / `apiKey` / `both` / `none`) so the credential model is visible at a glance.

Pi packages bumped `0.65.2 → 0.67.68` to pick up the `claude-opus-4-7` registry entry — without the bump, listing 4.7 would silently fall back to pi's global default model at spawn time. New `npm run pi:upgrade` script keeps Pi current going forward.

## Key decisions

- **Pi-OAuth listing path dropped, not adapted.** The plan assumed Pi's OAuth tokens would unlock `/v1/models`. They don't. Confirmed with curl: Anthropic returns `"OAuth authentication is currently not supported"`, OpenAI returns `Missing scopes: api.model.read`. Hybrid (live with API key, curated otherwise) is the only honest path — Option A from the mid-impl design discussion.
- **Curated lists track pi-ai's bundled registry, not Anthropic/OpenAI's published catalog.** Listing a model pi-ai doesn't know in `models.generated.js` causes the sidecar's `resolveModel` to return undefined, which today triggers a silent fallback to pi's global default model. Curated lists are bound by the dependency, not the upstream provider.
- **Authoritative subscription-supported set per provider.** OpenAI Codex IDs were pulled directly from the codex CLI's "Select Model and Effort" picker — the prefix-based heuristic produced false positives (gpt-5, gpt-5-mini, o1*, o3*, o4* aren't in the ChatGPT subscription). Anthropic still uses prefix-based matching (`claude-opus-4-` / `claude-sonnet-4-` / `claude-haiku-4-`) since its subscription cleanly tracks the gen-4 family.
- **Real cache-bust on Retry.** The original plan's Option A was "match today's no-op behavior". Mid-impl we changed to actually busting the cache via `forceRefresh` because today's no-op was the bug, not the spec.
- **Thinking-table fixes mirrored to pi-ai's actual checks.** `OPENAI_XHIGH_FAMILIES` now uses pi-ai's exact `supportsXhigh` rule (`gpt-5.2 / 5.3 / 5.4` includes-match), `ANTHROPIC_OPUS_46` is matched by `opus-4-` prefix so future minor versions inherit the right picker, and `minimal` was removed from the xhigh-family levels to match the codex CLI's effort picker.
- **Pi-ai bump bundled into this PR.** Tempted to defer it as a separate ticket but the curated list integrity depends on it — without the bump, listing the actual latest models (Opus 4.7) wasn't possible. Kept the cadence lightweight via the new `pi:upgrade` script.

## Files touched

- `src-tauri/src/models.rs` — full rewrite. `ModelCache` extended with `anthropic` + `openai_codex` slots, single `cached_or_fetch` helper, `pi_auth_entry_exists` reader, two live fetchers gated on env API keys, two curated fallback functions, subscription-supported sets, `AuthMode` enum, four-way auth-status matrix.
- `src-tauri/src/ws.rs` — `get_models` WS dispatch reads `forceRefresh` off args.
- `src/lib/bindings.ts` — regenerated. `getModels` signature now `(provider, forceRefresh)`; `ModelInfo.subscription` and `ProviderAuthStatus.authMode` added.
- `src/lib/ModelSelector.svelte` — `fixedModelId` and every branch guarding on it removed; `formatInvokeError` helper unpacks `ErrorDto.message`/`details`; refresh passes `forceRefresh=true`; per-row `SUB` / `API` tags; auth-mode chip in the status header; new CSS for both.
- `src/lib/providers.ts` — `REFRESHABLE_PROVIDERS` includes `anthropic` + `openai-codex`.
- `src/lib/thinking.ts` — `OPENAI_XHIGH_FAMILIES` matches pi-ai's `supportsXhigh`; `anthropicProfile` uses `opus-4-` / `sonnet-4-` prefix matching; `minimal` removed from `OPENAI_XHIGH`.
- `package.json` + `sidecar/package.json` — `pi-ai` and `pi-coding-agent` bumped to `^0.67.68`. Root adds `pi:upgrade` script.
- `CLAUDE.md` — documents `pi:upgrade` workflow + the maintenance ritual of revisiting curated lists in `models.rs` after each Pi bump.

## What was left out

- **Silent gpt-5.4 fallback bug.** When `resolveModel` returns undefined in the sidecar, the session is created without a model and pi uses its global default — surfaced via an error notification but the agent stays usable on the wrong model. Pre-existing, affects any unknown model id, deferred to a follow-up ticket. The curated list shrink + Pi bump removes the only way MON-81 could trigger it in practice.
- **OAuth refresh in Rust.** Pi's OAuth tokens can expire; once they do, `pi_auth_entry_exists` still returns true but spawn auth would fail. Not in scope — Pi sidecar refreshes on session start, and we're never using the OAuth token for listing anyway.
- **Renovate / Dependabot for pi packages.** Manual `npm run pi:upgrade` script ships now; auto-PR setup deferred until we feel the pain of forgetting.
- **Subscription-supported set for Anthropic kept prefix-based.** Could pull authoritative IDs from Claude Code's picker analogous to the Codex CLI move, but the gen-4 prefix is currently accurate and lower-maintenance.
