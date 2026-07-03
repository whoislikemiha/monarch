<script lang="ts">
  import { invoke } from "$lib/api";
  import type { ToolProps } from "../types";

  let { agentContext }: ToolProps = $props();

  // ── Supervisor state (global) ────────────────────────────────────────────
  let captainName = $state("");
  let captainPayload = $state("");
  let captainDirty = $state(false);
  let captainSaving = $state(false);
  let captainError = $state<string | null>(null);
  let captainLoading = $state(false);

  // ── Agent state (per-agent) ──────────────────────────────────────────────
  let shadowPayload = $state("");
  let shadowDirty = $state(false);
  let shadowSaving = $state(false);
  let shadowError = $state<string | null>(null);
  let shadowLoading = $state(false);

  // ── Token budget ──────────────────────────────────────────────────────────
  const COMBINED_TOKEN_CAP = 3000;
  const TOKEN_WARN_THRESHOLD = 2400;

  function estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }

  const captainTokens = $derived(estimateTokens(captainPayload));
  const shadowTokens = $derived(estimateTokens(shadowPayload));
  const combinedTokens = $derived(captainTokens + shadowTokens);
  const overBudget = $derived(combinedTokens > COMBINED_TOKEN_CAP);
  const nearBudget = $derived(combinedTokens > TOKEN_WARN_THRESHOLD && !overBudget);

  // ── Load supervisor identity on mount ────────────────────────────────────
  async function loadCaptain() {
    captainLoading = true;
    captainError = null;
    try {
      const row = await invoke<{ name: string; payload: string } | null>(
        "get_captain_identity",
        {},
      );
      if (row) {
        captainName = row.name;
        captainPayload = row.payload;
      }
    } catch (e) {
      captainError = String(e);
    } finally {
      captainLoading = false;
      captainDirty = false;
    }
  }

  // ── Load agent identity when agent changes ───────────────────────────────
  async function loadShadow(agentId: string) {
    shadowLoading = true;
    shadowError = null;
    try {
      const row = await invoke<{ payload: string } | null>("get_shadow_identity", {
        agentId,
      });
      shadowPayload = row?.payload ?? "";
    } catch (e) {
      shadowError = String(e);
    } finally {
      shadowLoading = false;
      shadowDirty = false;
    }
  }

  $effect(() => {
    loadCaptain();
  });

  $effect(() => {
    if (agentContext) {
      loadShadow(agentContext.agentId);
    } else {
      shadowPayload = "";
      shadowDirty = false;
    }
  });

  // ── Save handlers ─────────────────────────────────────────────────────────
  async function saveCaptain() {
    if (!captainDirty || overBudget) return;
    captainSaving = true;
    captainError = null;
    try {
      await invoke("upsert_captain_identity", {
        req: { name: captainName || "Supervisor", payload: captainPayload, editNote: null },
      });
      captainDirty = false;
    } catch (e) {
      captainError = String(e);
    } finally {
      captainSaving = false;
    }
  }

  async function saveShadow() {
    if (!agentContext || !shadowDirty || overBudget) return;
    shadowSaving = true;
    shadowError = null;
    try {
      await invoke("upsert_shadow_identity", {
        req: { agentId: agentContext.agentId, payload: shadowPayload, editNote: null },
      });
      shadowDirty = false;
    } catch (e) {
      shadowError = String(e);
    } finally {
      shadowSaving = false;
    }
  }

  function onCaptainInput() {
    captainDirty = true;
  }

  function onShadowInput() {
    shadowDirty = true;
  }
</script>

<div class="identity-tool">
  <!-- ── Token budget bar ──────────────────────────────────────────────── -->
  <div class="budget-row" class:warn={nearBudget} class:over={overBudget}>
    <span class="budget-label">Combined L1 budget</span>
    <span class="budget-value">{combinedTokens} / {COMBINED_TOKEN_CAP} est. tokens</span>
  </div>
  {#if overBudget}
    <p class="budget-warning">Over budget — reduce identity content before saving.</p>
  {:else if nearBudget}
    <p class="budget-warning warn-text">Approaching limit.</p>
  {/if}

  <!-- ── Supervisor identity (L1a) ──────────────────────────────────────── -->
  <div class="section">
    <div class="section-title">Supervisor (L1a — global)</div>

    {#if captainLoading}
      <p class="empty">Loading…</p>
    {:else}
      <div class="field-row">
        <label class="field-label" for="supervisor-name">Name</label>
        <input
          id="supervisor-name"
          class="field-input"
          type="text"
          bind:value={captainName}
          oninput={onCaptainInput}
          placeholder="Supervisor"
        />
      </div>

      <label class="field-label textarea-label" for="supervisor-payload">Identity</label>
      <textarea
        id="supervisor-payload"
        class="payload-area"
        rows={6}
        bind:value={captainPayload}
        oninput={onCaptainInput}
        placeholder="Who you are, your preferences, working style…"
      ></textarea>

      <div class="token-hint">~{captainTokens} tokens</div>

      {#if captainError}
        <p class="error-msg">{captainError}</p>
      {/if}

      <button
        class="save-btn"
        type="button"
        disabled={!captainDirty || captainSaving || overBudget}
        onclick={saveCaptain}
      >
        {captainSaving ? "Saving…" : captainDirty ? "Save" : "Saved"}
      </button>
    {/if}
  </div>

  <!-- ── Agent identity (L1b) ───────────────────────────────────────────── -->
  <div class="section">
    <div class="section-title">Agent (L1b — this agent)</div>

    {#if !agentContext}
      <p class="empty">No agent selected.</p>
    {:else if shadowLoading}
      <p class="empty">Loading…</p>
    {:else}
      <label class="field-label textarea-label" for="agent-payload">Identity</label>
      <textarea
        id="agent-payload"
        class="payload-area"
        rows={6}
        bind:value={shadowPayload}
        oninput={onShadowInput}
        placeholder="Agent-specific traits, persona, specialties…"
      ></textarea>

      <div class="token-hint">~{shadowTokens} tokens</div>

      {#if shadowError}
        <p class="error-msg">{shadowError}</p>
      {/if}

      <button
        class="save-btn"
        type="button"
        disabled={!shadowDirty || shadowSaving || overBudget}
        onclick={saveShadow}
      >
        {shadowSaving ? "Saving…" : shadowDirty ? "Save" : "Saved"}
      </button>
    {/if}
  </div>
</div>

<style>
  .identity-tool {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* Budget bar */
  .budget-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 10px;
    color: var(--text-muted);
    padding: 4px 6px;
    border-radius: 4px;
    background: var(--bg-panel-2);
    border: 1px solid var(--border-subtle);
    transition: border-color 0.15s, color 0.15s;
  }

  .budget-row.warn {
    border-color: var(--warning, #f2994a);
    color: var(--warning, #f2994a);
  }

  .budget-row.over {
    border-color: var(--error, #eb5757);
    color: var(--error, #eb5757);
  }

  .budget-label {
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
  }

  .budget-value {
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .budget-warning {
    margin: 0;
    font-size: 10px;
    color: var(--error, #eb5757);
  }

  .budget-warning.warn-text {
    color: var(--warning, #f2994a);
  }

  /* Sections */
  .section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .section-title {
    font-size: 9px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: 2px;
  }

  /* Fields */
  .field-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .field-label {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    white-space: nowrap;
  }

  .textarea-label {
    display: block;
    margin-bottom: 2px;
  }

  .field-input {
    flex: 1;
    padding: 4px 6px;
    background: var(--bg-input, var(--bg-panel-2));
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    color: var(--text-primary);
    font-family: inherit;
    font-size: 11px;
    outline: none;
  }

  .field-input:focus {
    border-color: var(--accent);
  }

  .payload-area {
    width: 100%;
    resize: vertical;
    padding: 6px 8px;
    background: var(--bg-input, var(--bg-panel-2));
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    color: var(--text-primary);
    font-family: inherit;
    font-size: 11px;
    line-height: 1.5;
    outline: none;
    box-sizing: border-box;
  }

  .payload-area:focus {
    border-color: var(--accent);
  }

  .token-hint {
    font-size: 9px;
    color: var(--text-muted);
    text-align: right;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  /* Save button */
  .save-btn {
    margin-top: 2px;
    padding: 5px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 10px;
    cursor: pointer;
    transition: background 0.15s;
    align-self: flex-start;
  }

  .save-btn:hover:not(:disabled) {
    background: var(--accent-bg-hover);
  }

  .save-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .empty {
    margin: 0;
    color: var(--text-muted);
    font-size: 11px;
    font-style: italic;
  }

  .error-msg {
    margin: 0;
    color: var(--error);
    font-size: 11px;
  }
</style>
