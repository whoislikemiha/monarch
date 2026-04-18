<script lang="ts">
  import { invoke } from "$lib/api";
  import type { AuthMode, ModelInfo, ProviderAuthStatus } from "./bindings";
  import { PROVIDERS, REFRESHABLE_PROVIDERS } from "./providers";
  import { supportsThinking } from "./thinking";
  import ThinkingPicker from "./ThinkingPicker.svelte";

  // Bindable surface. These values are the entire user-visible output of
  // the selector — the spawn form reads them on submit, the future runtime
  // switcher in AgentView will write them to `set_model`. `contextWindow` is
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
    provider = $bindable("openrouter"),
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

  let allModels: ModelInfo[] = $state([]);
  let modelsLoading = $state(false);
  let modelsError: string | null = $state(null);
  let modelFetchToken = 0;
  let authLoading = $state(false);
  let authStatus: ProviderAuthStatus | null = $state(null);
  let showDropdown = $state(false);
  let highlightedIndex = $state(-1);
  let modelInputEl: HTMLInputElement | undefined = $state(undefined);

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

  // Fuzzy filtered models — each space-separated term must match somewhere
  let filteredModels = $derived(() => {
    const query = model.toLowerCase().trim();
    if (!query) return allModels.slice(0, 50);
    const terms = query.split(/\s+/).filter(Boolean);
    return allModels
      .filter((m) => {
        const haystack = (m.id + " " + m.name).toLowerCase();
        return terms.every((t) => haystack.includes(t));
      })
      .slice(0, 50);
  });

  async function fetchModels(p: string, forceRefresh = false) {
    const token = ++modelFetchToken;
    modelsLoading = true;
    modelsError = null;
    try {
      const fetched = await invoke<ModelInfo[]>("get_models", {
        provider: p,
        forceRefresh,
      });
      if (token !== modelFetchToken) return; // stale — provider changed
      allModels = fetched;
    } catch (err) {
      if (token !== modelFetchToken) return;
      modelsError = formatInvokeError(err);
      allModels = [];
    } finally {
      if (token === modelFetchToken) {
        modelsLoading = false;
      }
    }
  }

  async function fetchAuthStatus(p: string) {
    authLoading = true;
    try {
      authStatus = await invoke<ProviderAuthStatus>("get_provider_auth_status", { provider: p });
    } catch {
      authStatus = null;
    }
    authLoading = false;
  }

  // Fetch models when provider changes. Reset UI state first so no stale
  // list/highlight bleeds in from the previous provider before the new
  // fetch resolves.
  $effect(() => {
    const p = provider;
    allModels = [];
    modelsError = null;
    model = "";
    showDropdown = false;
    highlightedIndex = -1;
    fetchModels(p);
    fetchAuthStatus(p);
  });

  // The LM Studio ModelInfo currently matching the typed id, if any. Drives
  // the read-only context display and the value sent on spawn.
  let selectedLmStudioModel = $derived.by(() => {
    if (provider !== "lmstudio") return undefined;
    const id = model.trim();
    if (!id) return undefined;
    return allModels.find((m) => m.id === id);
  });

  // Keep the bindable `contextWindow` prop in sync with the detected value.
  // LM Studio: ship the discovered length; anywhere else: clear it so a
  // stale LM Studio value doesn't leak into the next spawn payload.
  $effect(() => {
    const detected = selectedLmStudioModel?.contextWindow;
    contextWindow =
      provider === "lmstudio" && typeof detected === "number" && detected > 0
        ? Math.floor(detected)
        : undefined;
  });

  // Mirror the internal fetch lifecycle out through the bindable status prop.
  $effect(() => {
    modelsStatus = {
      loading: modelsLoading,
      error: modelsError,
      count: allModels.length,
    };
  });

  function refreshModels() {
    // Retry button always busts the cache so transient provider failures
    // (stale OAuth token, 5xx) can be retried without waiting out the TTL.
    fetchModels(provider, true);
  }

  function selectModel(m: ModelInfo) {
    model = m.id;
    showDropdown = false;
    highlightedIndex = -1;
  }

  function handleModelKeydown(e: KeyboardEvent) {
    const models = filteredModels();
    if (!showDropdown || models.length === 0) {
      if (e.key === "ArrowDown" && allModels.length > 0) {
        showDropdown = true;
        highlightedIndex = 0;
        e.preventDefault();
      }
      return;
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        highlightedIndex = Math.min(highlightedIndex + 1, models.length - 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        highlightedIndex = Math.max(highlightedIndex - 1, 0);
        break;
      case "Enter":
        if (highlightedIndex >= 0 && highlightedIndex < models.length) {
          e.preventDefault();
          e.stopPropagation();
          selectModel(models[highlightedIndex]);
        }
        break;
      case "Escape":
        // Close the dropdown and stop the event from reaching any parent
        // Escape handler (e.g. a dialog's "close on Escape"). Lets the
        // parent treat Escape as unconditional cancel without knowing
        // about our dropdown state.
        showDropdown = false;
        highlightedIndex = -1;
        e.preventDefault();
        e.stopPropagation();
        break;
    }
  }

  function formatCtxTokens(n: number): string {
    if (n >= 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + "M";
    if (n >= 1024) return (n / 1024).toFixed(n % 1024 === 0 ? 0 : 1) + "k";
    return String(n);
  }

  function authModeLabel(mode: AuthMode): string {
    switch (mode) {
      case "subscription": return "SUBSCRIPTION";
      case "apiKey": return "API KEY";
      case "both": return "SUB + API";
      case "none": return "NOT CONFIGURED";
    }
  }
</script>

<div class="section">
  <span class="label">Provider</span>
  <div class="preset-grid">
    {#each PROVIDERS as p}
      <button
        class="preset-btn"
        class:active={provider === p.value}
        onclick={() => (provider = p.value)}
      >
        {p.label}
      </button>
    {/each}
  </div>
</div>

{#if authLoading || authStatus}
  <div
    class="auth-status"
    class:ok={!!authStatus?.checked && !!authStatus?.configured}
    class:warn={!!authStatus?.checked && !authStatus?.configured}
    class:neutral={!authStatus?.checked}
  >
    <div class="auth-status-row">
      <span class="auth-status-label">
        {#if authLoading}
          Checking auth...
        {:else if authStatus?.checked && authStatus?.configured}
          Auth ready
        {:else if authStatus?.checked}
          Auth missing
        {:else}
          Auth not checked
        {/if}
      </span>
      {#if !authLoading && authStatus?.checked}
        <span class="auth-mode-chip" data-mode={authStatus.authMode}>
          {authModeLabel(authStatus.authMode)}
        </span>
      {/if}
    </div>
    {#if !authLoading && authStatus}
      <span class="auth-status-text">{authStatus.message}</span>
    {/if}
  </div>
{/if}

<div class="field model-field">
  <label class="label" for="model-input">Model</label>
  <div class="model-input-wrap">
    <input
      id="model-input"
      type="text"
      bind:this={modelInputEl}
      bind:value={model}
      placeholder={modelsLoading
        ? "Loading models..."
        : modelsError
          ? "Provider unreachable — see hint below"
          : allModels.length === 0
            ? "No models available"
            : "Search models..."}
      onfocus={() => (showDropdown = true)}
      onblur={() => setTimeout(() => (showDropdown = false), 200)}
      onkeydown={handleModelKeydown}
      oninput={() => { showDropdown = true; highlightedIndex = 0; }}
      autocomplete="off"
    />
    {#if modelsLoading}
      <span class="loading-indicator"></span>
    {:else if REFRESHABLE_PROVIDERS.has(provider)}
      <button
        class="refresh-btn"
        onmousedown={(e: MouseEvent) => { e.preventDefault(); refreshModels(); }}
        title="Refresh model list"
        type="button"
      >
        ↻
      </button>
    {/if}
    {#if showDropdown && filteredModels().length > 0}
      <div class="model-dropdown">
        {#each filteredModels() as m, i (m.id)}
          <button
            class="model-option"
            class:highlighted={i === highlightedIndex}
            class:selected={model === m.id}
            onmousedown={(e: MouseEvent) => { e.preventDefault(); selectModel(m); }}
            onmouseenter={() => (highlightedIndex = i)}
          >
            <div class="model-option-head">
              <span class="model-id">{m.id}</span>
              {#if m.subscription === true}
                <span class="model-tag tag-sub" title="Reachable via Pi subscription auth">SUB</span>
              {:else if m.subscription === false}
                <span class="model-tag tag-api" title="API-key only — Pi subscription cannot spawn this model">API</span>
              {/if}
            </div>
            <span class="model-name">{m.name}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
  {#if modelsError}
    <div class="model-error">
      <span class="model-error-label">Can't reach provider</span>
      <span class="model-error-text">{modelsError}</span>
      <button class="model-error-retry" onclick={refreshModels} type="button">
        Retry
      </button>
    </div>
  {:else if !modelsLoading && allModels.length === 0}
    <div class="field-hint">
      No models found for this provider.
    </div>
  {/if}
  {#if provider === "lmstudio"}
    <div class="lmstudio-context">
      <span class="label">
        Context window
        {#if selectedLmStudioModel?.contextWindow}
          <span class="lmstudio-ctx-value">
            {formatCtxTokens(selectedLmStudioModel.contextWindow)}
          </span>
        {/if}
      </span>
      <div class="field-hint">
        {#if selectedLmStudioModel?.contextWindow}
          Auto-detected from LM Studio.
        {:else if model.trim()}
          No context length reported for this model. Sidecar default will be used.
        {:else}
          Pick a loaded model above to see its context window.
        {/if}
      </div>
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
  .section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .label {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .preset-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }

  .preset-btn {
    padding: 6px 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .preset-btn:hover {
    background: var(--bg-panel-3);
  }

  .preset-btn.active {
    border-color: var(--accent);
    color: var(--accent);
  }

  .auth-status {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-panel-2);
  }

  .auth-status.ok {
    border-color: var(--auth-ok-border);
    background: var(--auth-ok-bg);
  }

  .auth-status.warn {
    border-color: var(--auth-warn-border);
    background: var(--auth-warn-bg);
  }

  .auth-status.neutral {
    border-color: var(--border-subtle);
  }

  .auth-status-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .auth-status-label {
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-primary);
  }

  .auth-status-text {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-secondary);
  }

  .auth-mode-chip {
    font-size: 9.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    letter-spacing: 0.6px;
    padding: 2px 6px;
    border-radius: 4px;
    border: 1px solid currentColor;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .auth-mode-chip[data-mode="subscription"] {
    color: var(--auth-ok-border, #4ade80);
    background: rgba(74, 222, 128, 0.08);
  }

  .auth-mode-chip[data-mode="apiKey"] {
    color: #60a5fa;
    background: rgba(96, 165, 250, 0.08);
  }

  .auth-mode-chip[data-mode="both"] {
    color: #c084fc;
    background: rgba(192, 132, 252, 0.08);
  }

  .auth-mode-chip[data-mode="none"] {
    color: var(--text-muted);
    background: transparent;
  }

  .model-field {
    position: relative;
  }

  .model-input-wrap {
    position: relative;
  }

  .loading-indicator {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    width: 12px;
    height: 12px;
    border: 2px solid var(--border-subtle);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: translateY(-50%) rotate(360deg); }
  }

  .refresh-btn {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    width: 22px;
    height: 22px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .refresh-btn:hover {
    background: var(--bg-panel-3);
    color: var(--accent);
  }

  .model-error {
    margin-top: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--model-error-border);
    background: var(--model-error-bg);
  }

  .model-error-label {
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--model-error-text);
  }

  .model-error-text {
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-secondary);
    overflow-wrap: anywhere;
  }

  .model-error-retry {
    align-self: flex-start;
    margin-top: 4px;
    padding: 4px 10px;
    border: 1px solid var(--model-error-retry-border);
    border-radius: 4px;
    background: transparent;
    color: var(--model-error-text);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
  }

  .model-error-retry:hover {
    background: var(--error-bg-subtle);
  }

  .model-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 200;
    max-height: 240px;
    overflow-y: auto;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-top: none;
    border-radius: 0 0 6px 6px;
    display: flex;
    flex-direction: column;
  }

  .model-option {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 10px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .model-option:hover,
  .model-option.highlighted {
    background: var(--bg-panel-3);
  }

  .model-option.selected {
    border-left: 2px solid var(--accent);
  }

  .model-option-head {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .model-id {
    color: var(--text-primary);
    font-size: 12px;
  }

  .model-name {
    color: var(--text-muted);
    font-size: 10px;
  }

  .model-tag {
    font-size: 8.5px;
    letter-spacing: 0.5px;
    padding: 1px 5px;
    border-radius: 3px;
    border: 1px solid currentColor;
    text-transform: uppercase;
    line-height: 1.2;
    flex-shrink: 0;
  }

  .model-tag.tag-sub {
    color: var(--auth-ok-border, #4ade80);
    background: rgba(74, 222, 128, 0.08);
  }

  .model-tag.tag-api {
    color: #fbbf24;
    background: rgba(251, 191, 36, 0.08);
  }

  .lmstudio-context {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 10px;
  }

  .lmstudio-context > .label {
    display: flex;
    align-items: baseline;
    gap: 8px;
    justify-content: space-between;
    text-transform: uppercase;
  }

  .lmstudio-ctx-value {
    color: var(--accent);
    font-size: 12px;
    text-transform: none;
    letter-spacing: 0;
  }

  .field-hint {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }

  input {
    width: 100%;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    padding: 8px 10px;
    outline: none;
  }

  input::placeholder {
    color: var(--text-muted);
    opacity: 0.6;
  }

  input:focus {
    border-color: var(--accent);
  }

  @media (max-width: 640px) {
    .preset-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
