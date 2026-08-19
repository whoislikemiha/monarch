<script lang="ts">
  import { invoke } from "$lib/api";
  import type { ModelInfo, ProviderAuthStatus } from "./bindings";
  import { PROVIDERS } from "./providers";
  import { supportsThinking } from "./thinking";
  import ThinkingPicker from "./ThinkingPicker.svelte";

  // Bindable surface. These values are the entire user-visible output of
  // the selector — the spawn form reads them on submit. `contextWindow` is
  // effectively read-only from the parent's perspective (populated only for
  // LM Studio when the discovered model reports a context length), but it's
  // bindable so the consumer can stamp it into their payload without pulling
  // the full model list. `modelsStatus` is read-only from the parent's side
  // too — it mirrors the async fetch lifecycle so the consumer can drive
  // gating copy without duplicating the fetch.
  export type ModelsStatus = {
    loading: boolean;
    error: string | null;
    count: number;
  };

  let {
    provider = $bindable("anthropic"),
    model = $bindable(""),
    thinkingLevel = $bindable("off"),
    contextWindow = $bindable<number | undefined>(undefined),
    modelsStatus = $bindable<ModelsStatus>({ loading: false, error: null, count: 0 }),
  }: {
    provider?: string;
    model?: string;
    thinkingLevel?: string;
    contextWindow?: number | undefined;
    modelsStatus?: ModelsStatus;
  } = $props();

  // One dropdown, all providers, grouped. Anthropic + Codex show only
  // subscription-reachable models — Monarch spawns those providers through
  // Pi's OAuth credential, so API-only entries would silently fall back to
  // pi's default model.
  const GROUPS: readonly { provider: string; label: string }[] = [
    { provider: "anthropic", label: "Anthropic subscription" },
    { provider: "openai-codex", label: "ChatGPT / Codex subscription" },
    { provider: "openrouter", label: "OpenRouter" },
    { provider: "lmstudio", label: "LM Studio" },
  ];

  const SUBSCRIPTION_ONLY = new Set(["anthropic", "openai-codex"]);
  // Per-group cap while no query is typed — OpenRouter alone lists hundreds.
  const UNFILTERED_GROUP_CAP = 12;
  const FILTERED_TOTAL_CAP = 60;

  let modelsByProvider = $state<Record<string, ModelInfo[]>>({});
  let pendingFetches = $state(0);
  // LM Studio is a local server that's frequently just not running — its
  // failure is expected and shows as an empty group, not an error.
  let fetchErrors = $state<Record<string, string>>({});
  let fetchGeneration = 0;

  let open = $state(false);
  let query = $state("");
  let highlightedIndex = $state(-1);
  let searchEl: HTMLInputElement | undefined = $state(undefined);
  let rootEl: HTMLDivElement | undefined = $state(undefined);

  let authStatus: ProviderAuthStatus | null = $state(null);

  // The Tauri invoke wrapper rejects with the serialised `ErrorDto`
  // (`{ kind, message, details }`) — not a real Error instance. Plain
  // `String(err)` would give "[object Object]"; pull the message field
  // (and details if any) so the user sees what actually failed.
  function formatInvokeError(err: unknown): string {
    if (err instanceof Error) return err.message;
    if (err && typeof err === "object") {
      const dto = err as { message?: unknown; details?: unknown };
      const msg = typeof dto.message === "string" ? dto.message : "";
      const details = typeof dto.details === "string" ? dto.details : "";
      if (msg && details) return `${msg} (${details})`;
      if (msg) return msg;
      if (details) return details;
    }
    return String(err);
  }

  async function fetchAll(forceRefresh = false) {
    const generation = ++fetchGeneration;
    fetchErrors = {};
    pendingFetches = GROUPS.length;
    for (const g of GROUPS) {
      invoke<ModelInfo[]>("get_models", { provider: g.provider, forceRefresh })
        .then((fetched) => {
          if (generation !== fetchGeneration) return;
          modelsByProvider = {
            ...modelsByProvider,
            [g.provider]: SUBSCRIPTION_ONLY.has(g.provider)
              ? fetched.filter((m) => m.subscription !== false)
              : fetched,
          };
        })
        .catch((err) => {
          if (generation !== fetchGeneration) return;
          modelsByProvider = { ...modelsByProvider, [g.provider]: [] };
          if (g.provider !== "lmstudio") {
            fetchErrors = { ...fetchErrors, [g.provider]: formatInvokeError(err) };
          }
        })
        .finally(() => {
          if (generation === fetchGeneration) pendingFetches -= 1;
        });
    }
  }

  $effect(() => {
    fetchAll();
  });

  // Auth status for the selected model's provider — only meaningful for the
  // two Pi-subscription providers.
  $effect(() => {
    const p = provider;
    if (!SUBSCRIPTION_ONLY.has(p)) {
      authStatus = null;
      return;
    }
    invoke<ProviderAuthStatus>("get_provider_auth_status", { provider: p })
      .then((s) => {
        if (provider === p) authStatus = s;
      })
      .catch(() => {
        authStatus = null;
      });
  });

  type GroupView = { provider: string; label: string; models: ModelInfo[] };

  // Fuzzy filter — each space-separated term must match id, name, or the
  // group label somewhere.
  let groupViews = $derived.by<GroupView[]>(() => {
    const q = query.toLowerCase().trim();
    const terms = q.split(/\s+/).filter(Boolean);
    let remaining = FILTERED_TOTAL_CAP;
    const out: GroupView[] = [];
    for (const g of GROUPS) {
      const all = modelsByProvider[g.provider] ?? [];
      let matched = terms.length
        ? all.filter((m) => {
            const haystack = `${m.id} ${m.name} ${g.label}`.toLowerCase();
            return terms.every((t) => haystack.includes(t));
          })
        : all.slice(0, UNFILTERED_GROUP_CAP);
      if (terms.length) {
        matched = matched.slice(0, remaining);
        remaining -= matched.length;
      }
      if (matched.length > 0) out.push({ provider: g.provider, label: g.label, models: matched });
    }
    return out;
  });

  // Flattened row list for keyboard navigation.
  let flatRows = $derived(groupViews.flatMap((g) => g.models.map((m) => ({ g, m }))));

  let selectedInfo = $derived.by(() => {
    const list = modelsByProvider[provider] ?? [];
    return list.find((m) => m.id === model);
  });

  let totalCount = $derived(GROUPS.reduce((n, g) => n + (modelsByProvider[g.provider]?.length ?? 0), 0));
  let anyLoading = $derived(pendingFetches > 0);
  let firstError = $derived(Object.values(fetchErrors)[0] ?? null);

  // Mirror the internal fetch lifecycle out through the bindable status prop.
  $effect(() => {
    modelsStatus = { loading: anyLoading, error: firstError, count: totalCount };
  });

  // LM Studio ships a detected context length; anywhere else clear it so a
  // stale value doesn't leak into the next spawn payload.
  $effect(() => {
    const detected = provider === "lmstudio" ? selectedInfo?.contextWindow : undefined;
    contextWindow = typeof detected === "number" && detected > 0 ? Math.floor(detected) : undefined;
  });

  function providerLabel(p: string): string {
    return PROVIDERS.find((x) => x.value === p)?.label ?? p;
  }

  function openDropdown() {
    open = true;
    query = "";
    highlightedIndex = -1;
    queueMicrotask(() => searchEl?.focus());
  }

  function closeDropdown() {
    open = false;
    query = "";
    highlightedIndex = -1;
  }

  function toggleDropdown() {
    if (open) closeDropdown();
    else openDropdown();
  }

  function selectRow(row: { g: GroupView; m: ModelInfo }) {
    provider = row.g.provider;
    model = row.m.id;
    closeDropdown();
  }

  function handleSearchKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        highlightedIndex = Math.min(highlightedIndex + 1, flatRows.length - 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        highlightedIndex = Math.max(highlightedIndex - 1, 0);
        break;
      case "Enter":
        if (highlightedIndex >= 0 && highlightedIndex < flatRows.length) {
          e.preventDefault();
          e.stopPropagation();
          selectRow(flatRows[highlightedIndex]);
        } else if (flatRows.length === 1) {
          e.preventDefault();
          e.stopPropagation();
          selectRow(flatRows[0]);
        }
        break;
      case "Escape":
        // Close and stop the event from reaching any parent Escape handler
        // (e.g. a dialog's "close on Escape") — lets the parent treat Escape
        // as unconditional cancel without knowing about our dropdown state.
        e.preventDefault();
        e.stopPropagation();
        closeDropdown();
        break;
    }
  }

  function handleFocusOut(e: FocusEvent) {
    // Close when focus leaves the whole selector (trigger + popover).
    const next = e.relatedTarget as Node | null;
    if (open && (!next || !rootEl?.contains(next))) closeDropdown();
  }

  function refreshModels() {
    // Refresh always busts the cache so transient provider failures (stale
    // OAuth token, 5xx) can be retried without waiting out the TTL.
    fetchAll(true);
    searchEl?.focus();
  }

  function formatCtxTokens(n: number): string {
    if (n >= 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + "M";
    if (n >= 1024) return (n / 1024).toFixed(n % 1024 === 0 ? 0 : 1) + "k";
    return String(n);
  }
</script>

<div class="field model-field" bind:this={rootEl} onfocusout={handleFocusOut}>
  <span class="label" id="model-picker-label">Model</span>
  <button
    class="trigger"
    class:placeholder={!model}
    type="button"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-labelledby="model-picker-label"
    onclick={toggleDropdown}
  >
    {#if model}
      <span class="trigger-model">{selectedInfo?.name ?? model}</span>
      <span class="trigger-provider">{providerLabel(provider)}</span>
    {:else if anyLoading && totalCount === 0}
      <span class="trigger-empty">Loading models…</span>
    {:else}
      <span class="trigger-empty">Select model…</span>
    {/if}
    <span class="trigger-caret" aria-hidden="true">▾</span>
  </button>

  {#if open}
    <div class="popover" role="listbox">
      <div class="search-wrap">
        <input
          class="search"
          type="text"
          bind:this={searchEl}
          bind:value={query}
          placeholder="Search models"
          onkeydown={handleSearchKeydown}
          oninput={() => (highlightedIndex = flatRows.length > 0 ? 0 : -1)}
          autocomplete="off"
        />
        {#if anyLoading}
          <span class="loading-indicator"></span>
        {/if}
      </div>

      <div class="options">
        {#each groupViews as g (g.provider)}
          <div class="group-header">{g.label}</div>
          {#each g.models as m (g.provider + m.id)}
            {@const flatIdx = flatRows.findIndex((r) => r.g.provider === g.provider && r.m.id === m.id)}
            <button
              class="option"
              class:highlighted={flatIdx === highlightedIndex}
              type="button"
              role="option"
              aria-selected={provider === g.provider && model === m.id}
              onmousedown={(e: MouseEvent) => { e.preventDefault(); selectRow({ g, m }); }}
              onmouseenter={() => (highlightedIndex = flatIdx)}
            >
              <span class="option-name" class:mono={m.name === m.id}>{m.name}</span>
              {#if m.name !== m.id}
                <span class="option-id">{m.id}</span>
              {/if}
              {#if provider === g.provider && model === m.id}
                <span class="option-check" aria-hidden="true">✓</span>
              {/if}
            </button>
          {/each}
        {/each}
        {#if flatRows.length === 0}
          <div class="empty-note">
            {#if anyLoading}
              Loading models…
            {:else if query.trim()}
              No models match "{query.trim()}".
            {:else}
              No models available.
            {/if}
          </div>
        {/if}
      </div>

      <div class="popover-footer">
        <button class="footer-btn" type="button" onmousedown={(e: MouseEvent) => { e.preventDefault(); refreshModels(); }}>
          <span class="footer-icon" aria-hidden="true">↻</span> Refresh models
        </button>
      </div>
    </div>
  {/if}

  {#if firstError}
    <div class="model-error">
      <span class="model-error-label">Some model lists failed to load</span>
      <span class="model-error-text">{firstError}</span>
      <button class="model-error-retry" onclick={() => fetchAll(true)} type="button">Retry</button>
    </div>
  {/if}

  {#if authStatus && !authStatus.configured}
    <div class="auth-warn">
      <span class="auth-warn-label">Auth missing</span>
      <span class="auth-warn-text">{authStatus.message}</span>
    </div>
  {/if}

  {#if provider === "lmstudio" && model}
    <div class="field-hint">
      {#if selectedInfo?.contextWindow}
        Context window {formatCtxTokens(selectedInfo.contextWindow)} — auto-detected from LM Studio.
      {:else}
        No context length reported for this model. Sidecar default will be used.
      {/if}
    </div>
  {/if}
</div>

{#if supportsThinking(provider, model)}
  <div class="field">
    <span class="label">Thinking</span>
    <ThinkingPicker {provider} {model} bind:value={thinkingLevel} />
  </div>
{/if}

<style>
  /* Design system: Inter for labels/copy, mono only for model ids.
     Foundation tokens throughout — no shadows, depth via elevation + border. */
  .label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex: 1;
    min-width: 0;
  }

  .model-field {
    position: relative;
  }

  .trigger {
    font: inherit;
    font-size: 12.5px;
    display: flex;
    align-items: center;
    gap: var(--s2);
    width: 100%;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    color: var(--text-primary);
    padding: 7px var(--s3);
    cursor: pointer;
    text-align: left;
    transition: border-color 0.14s, background 0.14s;
  }

  .trigger:hover {
    background: var(--bg-overlay);
    border-color: var(--border-strong);
  }

  .trigger:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
    border-color: var(--accent);
  }

  .trigger-model {
    font-weight: 500;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .trigger-provider {
    font-size: 10.5px;
    color: var(--text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .trigger-empty {
    color: var(--text-muted);
  }

  .trigger-caret {
    margin-left: auto;
    color: var(--text-muted);
    font-size: 10px;
    flex-shrink: 0;
  }

  .popover {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 200;
    margin-top: 4px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-md);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .search-wrap {
    position: relative;
    padding: var(--s2);
    border-bottom: 1px solid var(--border-subtle);
  }

  .search {
    font: inherit;
    font-size: 12.5px;
    width: 100%;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    color: var(--text-primary);
    padding: 5px var(--s3);
  }

  .search::placeholder {
    color: var(--text-muted);
  }

  .search:focus {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
    border-color: var(--accent);
  }

  .loading-indicator {
    position: absolute;
    right: 16px;
    top: 50%;
    transform: translateY(-50%);
    width: 12px;
    height: 12px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--accent);
    border-radius: var(--r-full);
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: translateY(-50%) rotate(360deg); }
  }

  @media (prefers-reduced-motion: reduce) {
    .loading-indicator { animation: none; }
  }

  .options {
    max-height: 280px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: var(--s1) 0;
  }

  .group-header {
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--text-muted);
    padding: var(--s2) var(--s3) 3px;
    position: sticky;
    top: 0;
    background: var(--bg-overlay);
  }

  .option {
    display: flex;
    align-items: baseline;
    gap: var(--s2);
    padding: 5px var(--s3);
    border: none;
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .option:hover,
  .option.highlighted {
    background: var(--bg-raised);
  }

  .option-name {
    font-size: 12px;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .option-name.mono,
  .option-id {
    font-family: "JetBrains Mono", monospace;
  }

  .option-id {
    font-size: 10px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .option-check {
    margin-left: auto;
    color: var(--accent);
    font-size: 11px;
    flex-shrink: 0;
  }

  .empty-note {
    font-size: 11.5px;
    color: var(--text-muted);
    padding: var(--s3);
  }

  .popover-footer {
    border-top: 1px solid var(--border-subtle);
    padding: var(--s1);
  }

  .footer-btn {
    font: inherit;
    font-size: 11.5px;
    display: flex;
    align-items: center;
    gap: var(--s2);
    width: 100%;
    padding: 5px var(--s2);
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    text-align: left;
  }

  .footer-btn:hover {
    background: var(--bg-raised);
    color: var(--text-primary);
  }

  .footer-icon {
    color: var(--text-muted);
  }

  .model-error {
    margin-top: var(--s2);
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: var(--s2) var(--s3);
    border-radius: var(--r-md);
    border: 1px solid color-mix(in srgb, var(--status-error) 38%, transparent);
    background: color-mix(in srgb, var(--status-error) 7%, var(--bg-raised));
  }

  .model-error-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--status-error);
  }

  .model-error-text {
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-secondary);
    overflow-wrap: anywhere;
  }

  .model-error-retry {
    align-self: flex-start;
    margin-top: 2px;
    font: inherit;
    font-size: 11px;
    font-weight: 600;
    padding: 3px var(--s2);
    border: 1px solid color-mix(in srgb, var(--status-error) 45%, transparent);
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--status-error);
    cursor: pointer;
  }

  .model-error-retry:hover {
    background: color-mix(in srgb, var(--status-error) 14%, transparent);
  }

  .auth-warn {
    margin-top: var(--s2);
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: var(--s2) var(--s3);
    border-radius: var(--r-md);
    border: 1px solid color-mix(in srgb, var(--status-warning) 35%, transparent);
    background: color-mix(in srgb, var(--status-warning) 7%, var(--bg-raised));
  }

  .auth-warn-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--status-warning);
  }

  .auth-warn-text {
    font-size: 11.5px;
    line-height: 1.45;
    color: var(--text-secondary);
  }

  .field-hint {
    margin-top: 4px;
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.5;
  }
</style>
