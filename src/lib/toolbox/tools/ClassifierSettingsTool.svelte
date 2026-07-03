<script lang="ts">
  /**
   * MON-82 — classifier configuration. Global (one config for every agent).
   * The system prompt is displayed read-only; editing is out of scope for
   * Slice 1. The toggle + model pickers write through
   * `classifier_set_config`, which persists `~/.config/monarch/classifier.toml`.
   */
  import { invoke } from "$lib/api";
  import type { ToolProps } from "../types";
  import type { ResolvedClassifierConfig } from "$lib/bindings";

  // agentContext is unused — this is a global setting, not per-agent. Kept
  // in the signature so the tool can be mounted via the standard toolbox
  // contract.
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  let { agentContext: _agentContext }: ToolProps = $props();

  let config = $state<ResolvedClassifierConfig | null>(null);
  let configPath = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function refresh() {
    loading = true;
    error = null;
    try {
      config = await invoke<ResolvedClassifierConfig>(
        "classifier_get_config",
      );
      configPath = await invoke<string>("classifier_get_config_path");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function save() {
    if (!config) return;
    loading = true;
    error = null;
    try {
      config = await invoke<ResolvedClassifierConfig>(
        "classifier_set_config",
        {
          config: {
            enabled: config.enabled,
            primary: config.primary,
            fallback: config.fallback,
            timeoutMs: config.timeoutMs,
          },
        },
      );
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void refresh();
  });

  let hasFallback = $derived(!!config?.fallback);

  function setFallback(enabled: boolean) {
    if (!config) return;
    if (enabled) {
      config.fallback = config.fallback ?? {
        provider: "lmstudio",
        model: "qwen3-4b-instruct",
      };
    } else {
      config.fallback = null;
    }
  }
</script>

<div class="classifier-tool">
  {#if loading && !config}
    <p class="empty">Loading…</p>
  {:else if config}
    <label class="toggle">
      <input
        type="checkbox"
        bind:checked={config.enabled}
        onchange={save}
        disabled={loading}
      />
      <span>Enabled — classify every user turn</span>
    </label>

    <div class="section">
      <div class="section-title">Primary provider</div>
      <div class="row">
        <span class="label">Provider</span>
        <input
          class="input"
          type="text"
          bind:value={config.primary.provider}
          onchange={save}
          disabled={loading || !config.enabled}
        />
      </div>
      <div class="row">
        <span class="label">Model</span>
        <input
          class="input"
          type="text"
          bind:value={config.primary.model}
          onchange={save}
          disabled={loading || !config.enabled}
        />
      </div>
    </div>

    <label class="toggle">
      <input
        type="checkbox"
        checked={hasFallback}
        onchange={(e) => {
          setFallback((e.currentTarget as HTMLInputElement).checked);
          void save();
        }}
        disabled={loading || !config.enabled}
      />
      <span>Use fallback on primary failure</span>
    </label>

    {#if hasFallback && config.fallback}
      <div class="section">
        <div class="section-title">Fallback provider</div>
        <div class="row">
          <span class="label">Provider</span>
          <input
            class="input"
            type="text"
            bind:value={config.fallback.provider}
            onchange={save}
            disabled={loading || !config.enabled}
          />
        </div>
        <div class="row">
          <span class="label">Model</span>
          <input
            class="input"
            type="text"
            bind:value={config.fallback.model}
            onchange={save}
            disabled={loading || !config.enabled}
          />
        </div>
      </div>
    {/if}

    <div class="row">
      <span class="label">Timeout (ms)</span>
      <input
        class="input narrow"
        type="number"
        min="500"
        max="30000"
        step="500"
        bind:value={config.timeoutMs}
        onchange={save}
        disabled={loading || !config.enabled}
      />
    </div>

    <div class="section">
      <div class="section-title">System prompt · read-only</div>
      <pre class="prompt mono">{config.systemPrompt}</pre>
    </div>

    {#if configPath}
      <div class="row">
        <span class="label">File</span>
        <span class="value mono">{configPath}</span>
      </div>
    {/if}
  {/if}

  {#if error}
    <pre class="error">{error}</pre>
  {/if}
</div>

<style>
  .classifier-tool {
    display: flex;
    flex-direction: column;
    gap: var(--s3);
    padding: var(--s3);
  }
  .mono { font-family: "JetBrains Mono", monospace; }
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
  }
  .section-title {
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.14em;
    font-size: 10px;
    font-weight: 600;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: var(--s2);
    font-size: 12px;
    color: var(--text-primary);
    cursor: pointer;
    user-select: none;
  }
  .toggle input[type="checkbox"] {
    accent-color: var(--accent);
    margin: 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s2);
    font-size: 11px;
  }
  .label {
    color: var(--text-muted);
    flex: none;
    width: 72px;
  }
  .value {
    color: var(--text-secondary);
    font-size: 10px;
    word-break: break-all;
    text-align: right;
  }
  .input {
    flex: 1 1 auto;
    min-width: 0;
    font: inherit;
    font-family: "JetBrains Mono", monospace;
    font-size: 11px;
    padding: 4px var(--s2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--bg-raised);
    color: var(--text-primary);
    transition: border-color 0.14s, background 0.14s;
  }
  .input:focus {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
    border-color: var(--accent);
    background: var(--bg-overlay);
  }
  .input:disabled { opacity: 0.5; }
  .input.narrow {
    flex: 0 0 6rem;
  }
  .prompt {
    margin: 0;
    padding: var(--s2) var(--s3);
    background: var(--bg-sink);
    border: 1px solid var(--border-subtle);
    border-radius: var(--r-sm);
    color: var(--text-secondary);
    font-size: 10px;
    line-height: 1.6;
    white-space: pre-wrap;
    max-height: 14rem;
    overflow: auto;
  }
  .empty {
    color: var(--text-muted);
    font-size: 11px;
    padding: var(--s2) 0;
  }
  .error {
    margin: 0;
    padding: var(--s2) var(--s3);
    border: 1px solid color-mix(in srgb, var(--status-error) 38%, transparent);
    background: color-mix(in srgb, var(--status-error) 7%, var(--bg-raised));
    border-radius: var(--r-sm);
    color: var(--status-error);
    font-size: 10px;
    white-space: pre-wrap;
  }
</style>
