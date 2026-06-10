<script lang="ts">
  /**
   * MON-99 Slice A — Memory tab in the Settings dialog. Global config
   * (one Memory config for every agent), persisted to
   * `~/.config/monarch/memory.toml` via `memory_set_config`.
   *
   * D4 (locked in `thoughts/plan/MON-99.md`): the Save button is gated
   * on the embedder being initialised — without a downloaded model we
   * cannot embed memories at insert time, so configuring the Keeper
   * before the embedder is ready is a footgun. The "Download model"
   * button is the only available action while status is false.
   */
  import { invoke } from "$lib/api";
  import type {
    MemoryConfig,
    ResolvedMemoryConfig,
  } from "$lib/bindings";

  let resolved = $state<ResolvedMemoryConfig | null>(null);
  let configPath = $state<string | null>(null);
  let embedderReady = $state(false);
  let loading = $state(false);
  let downloading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);

  // Local form state — only the fields the captain edits in Slice A.
  // Embedding model id and models_dir are display-only (defaults are fine).
  let keeperEnabled = $state(false);
  let provider = $state("anthropic");
  let model = $state("claude-haiku-4-5");
  let topK = $state(5);
  // MON-100: continuous-compaction thresholds. Defaults match
  // memory_config::DEFAULT_SOFT/HARD_THRESHOLD_TOKENS until refresh()
  // overwrites with the resolved view from the backend.
  let softThresholdTokens = $state(25_000);
  let hardThresholdTokens = $state(30_000);
  let promptExpanded = $state(false);

  let dirty = $derived(
    !!resolved &&
      (keeperEnabled !== !!resolved.keeper ||
        (keeperEnabled &&
          (provider !== resolved.keeper?.provider ||
            model !== resolved.keeper?.model)) ||
        topK !== resolved.topK ||
        softThresholdTokens !== resolved.softThresholdTokens ||
        hardThresholdTokens !== resolved.hardThresholdTokens),
  );

  async function refresh() {
    loading = true;
    error = null;
    try {
      resolved = await invoke<ResolvedMemoryConfig>("memory_get_config");
      configPath = await invoke<string>("memory_get_config_path");
      embedderReady = await invoke<boolean>("memory_index_status");

      // Seed the form from the resolved view.
      keeperEnabled = !!resolved.keeper;
      if (resolved.keeper) {
        provider = resolved.keeper.provider;
        model = resolved.keeper.model;
      }
      topK = resolved.topK;
      softThresholdTokens = resolved.softThresholdTokens;
      hardThresholdTokens = resolved.hardThresholdTokens;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function downloadModel() {
    downloading = true;
    error = null;
    try {
      await invoke("memory_download_and_init");
      embedderReady = await invoke<boolean>("memory_index_status");
    } catch (e) {
      error = String(e);
    } finally {
      downloading = false;
    }
  }

  async function save() {
    if (!resolved || !embedderReady) return;
    saving = true;
    error = null;
    try {
      const payload: MemoryConfig = {
        keeper: keeperEnabled ? { provider, model } : null,
        topK,
        softThresholdTokens,
        hardThresholdTokens,
      };
      resolved = await invoke<ResolvedMemoryConfig>("memory_set_config", {
        config: payload,
      });
      // Re-seed thresholds from the round-tripped resolved view in case the
      // backend clamped or substituted defaults.
      softThresholdTokens = resolved.softThresholdTokens;
      hardThresholdTokens = resolved.hardThresholdTokens;
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  $effect(() => {
    void refresh();
  });
</script>

<div class="memory-settings">
  {#if loading && !resolved}
    <p class="empty">Loading…</p>
  {:else if resolved}
    <section class="card">
      <div class="card-title">Embedding model</div>
      <div class="row">
        <span class="label">Model</span>
        <span class="value mono">{resolved.embeddingModelId}</span>
      </div>
      <div class="row">
        <span class="label">Status</span>
        <span class="value">
          {#if embedderReady}
            <span class="status-ok">Ready</span>
          {:else}
            <span class="status-pending">Not downloaded</span>
          {/if}
        </span>
      </div>
      <div class="row">
        <span class="label">Path</span>
        <span class="value mono small">{resolved.modelsDir}</span>
      </div>
      {#if !embedderReady}
        <button
          class="btn-primary"
          onclick={downloadModel}
          disabled={downloading}
        >
          {downloading ? "Downloading…" : "Download model"}
        </button>
        <p class="hint">
          ~127 MiB on first download. Required before saving the
          Keeper configuration.
        </p>
      {/if}
    </section>

    <section class="card" class:disabled={!embedderReady}>
      <div class="card-title">Keeper</div>
      <label class="toggle">
        <input
          type="checkbox"
          bind:checked={keeperEnabled}
          disabled={!embedderReady || saving}
        />
        <span>Enable memory formation at objective-close</span>
      </label>
      <div class="row">
        <span class="label">Provider</span>
        <input
          class="input"
          type="text"
          bind:value={provider}
          disabled={!embedderReady || !keeperEnabled || saving}
          placeholder="anthropic"
        />
      </div>
      <div class="row">
        <span class="label">Model</span>
        <input
          class="input"
          type="text"
          bind:value={model}
          disabled={!embedderReady || !keeperEnabled || saving}
          placeholder="claude-haiku-4-5"
        />
      </div>
      {#if !keeperEnabled}
        <p class="hint">
          Without a Keeper model, the agent loop runs unchanged and no
          memories form at objective-close.
        </p>
      {/if}
      <div class="row">
        <span class="label">Soft trigger (tokens)</span>
        <input
          class="input narrow"
          type="number"
          min="1000"
          step="1000"
          bind:value={softThresholdTokens}
          disabled={!embedderReady || !keeperEnabled || saving}
        />
      </div>
      <div class="row">
        <span class="label">Hard trigger (tokens)</span>
        <input
          class="input narrow"
          type="number"
          min="1000"
          step="1000"
          bind:value={hardThresholdTokens}
          disabled={!embedderReady || !keeperEnabled || saving}
        />
      </div>
      <p class="hint">
        Soft fires at the next turn-end once activity since the last successful
        Keeper run crosses this; hard forces a clean cut at any message-end.
      </p>

      <details bind:open={promptExpanded} class="prompt-details">
        <summary class="prompt-summary">
          Keeper system prompt (read-only, ships from code)
        </summary>
        <pre class="prompt-text">{resolved.keeperSystemPrompt}</pre>
      </details>
    </section>

    <section class="card">
      <div class="card-title">Retrieval</div>
      <div class="row">
        <span class="label">Top-K per turn</span>
        <input
          class="input narrow"
          type="number"
          min="1"
          max="20"
          step="1"
          bind:value={topK}
          disabled={!embedderReady || saving}
        />
      </div>
    </section>

    <div class="actions">
      <button
        class="btn-primary"
        onclick={save}
        disabled={!embedderReady || !dirty || saving}
      >
        {saving ? "Saving…" : "Save"}
      </button>
      {#if !embedderReady}
        <span class="hint inline">Download the embedding model first.</span>
      {:else if !dirty}
        <span class="hint inline">No changes to save.</span>
      {/if}
    </div>

    {#if configPath}
      <div class="row mono small footer-row">
        <span class="label">File</span>
        <span class="value">{configPath}</span>
      </div>
    {/if}
  {/if}

  {#if error}
    <pre class="error">{error}</pre>
  {/if}
</div>

<style>
  .memory-settings {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    background: var(--bg-panel-2);
  }
  .card.disabled {
    opacity: 0.55;
  }
  .card-title {
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    cursor: pointer;
    color: var(--text-primary);
  }
  .toggle input:disabled {
    cursor: not-allowed;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 11px;
  }
  .row.footer-row {
    padding-top: 4px;
  }
  .label {
    color: var(--text-muted);
  }
  .value {
    color: var(--text-primary);
    text-align: right;
  }
  .value.mono {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    word-break: break-all;
  }
  .value.small,
  .row.mono.small {
    font-size: 10px;
  }
  .status-ok {
    color: var(--accent);
  }
  .status-pending {
    color: var(--text-muted);
  }
  .input {
    flex: 1 1 auto;
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: var(--bg-panel);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 11px;
  }
  .input.narrow {
    flex: 0 0 5rem;
  }
  .input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn-primary {
    align-self: flex-start;
    padding: 6px 14px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent-bg-hover);
    color: var(--accent);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    transition: background 0.15s;
  }
  .btn-primary:hover:not(:disabled) {
    background: var(--accent);
    color: var(--bg-panel);
  }
  .btn-primary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .hint {
    margin: 0;
    color: var(--text-muted);
    font-size: 10px;
    line-height: 1.4;
  }
  .hint.inline {
    margin: 0;
  }
  .empty {
    color: var(--text-muted);
    font-style: italic;
    font-size: 11px;
  }
  .error {
    margin: 0;
    padding: 6px 8px;
    background: var(--error-bg-faint);
    color: var(--error-light);
    font-size: 10px;
    white-space: pre-wrap;
    border-radius: 4px;
  }
  .prompt-details {
    margin-top: 4px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: var(--bg-panel);
  }
  .prompt-summary {
    padding: 6px 8px;
    font-size: 10px;
    color: var(--text-muted);
    cursor: pointer;
    user-select: none;
  }
  .prompt-summary:hover {
    color: var(--text-secondary);
  }
  .prompt-text {
    margin: 0;
    padding: 8px;
    border-top: 1px solid var(--border-subtle);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    font-size: 10px;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 320px;
    overflow-y: auto;
  }
</style>
