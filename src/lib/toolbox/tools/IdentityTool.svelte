<script lang="ts">
  /**
   * Identity editors: supervisor (L1a, global) + agent (L1b, per-agent).
   * Both payloads are injected into every system prompt, so a combined token
   * budget meter guards the pair. DB-backed — works for sleeping agents.
   */
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
  const budgetPct = $derived(Math.min(100, (combinedTokens / COMBINED_TOKEN_CAP) * 100));

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
</script>

<div class="identity">
  <!-- combined system-prompt budget -->
  <div class="meter" class:warn={nearBudget} class:over={overBudget}>
    <div class="top">
      <span class="lab">Prompt budget</span>
      <span class="val mono">{combinedTokens} / {COMBINED_TOKEN_CAP} tok</span>
    </div>
    <div class="track"><div class="fill" style="width:{budgetPct}%"></div></div>
    {#if overBudget}
      <p class="note over-note">Over budget — trim identity content before saving.</p>
    {:else if nearBudget}
      <p class="note warn-note">Approaching the combined limit.</p>
    {/if}
  </div>

  <!-- supervisor identity (L1a) -->
  <section class="block">
    <div class="bh">
      <span class="bt">Supervisor</span>
      <span class="rule"></span>
      <span class="bm mono">global · ~{captainTokens} tok</span>
    </div>

    {#if captainLoading}
      <div class="blank">Loading…</div>
    {:else}
      <div class="field">
        <label for="supervisor-name">Name</label>
        <input
          id="supervisor-name"
          class="input"
          type="text"
          bind:value={captainName}
          oninput={() => (captainDirty = true)}
          placeholder="Supervisor"
        />
      </div>

      <div class="field">
        <label for="supervisor-payload">Identity</label>
        <textarea
          id="supervisor-payload"
          class="textarea"
          rows={6}
          bind:value={captainPayload}
          oninput={() => (captainDirty = true)}
          placeholder="Who you are, your preferences, working style…"
        ></textarea>
      </div>

      {#if captainError}<p class="err">{captainError}</p>{/if}

      <button
        class="save"
        class:dirty={captainDirty}
        type="button"
        disabled={!captainDirty || captainSaving || overBudget}
        onclick={saveCaptain}
      >
        {captainSaving ? "Saving…" : captainDirty ? "Save" : "Saved"}
      </button>
    {/if}
  </section>

  <!-- agent identity (L1b) -->
  <section class="block">
    <div class="bh">
      <span class="bt">Agent</span>
      <span class="rule"></span>
      <span class="bm mono">
        {agentContext ? `${agentContext.agent.name} · ~${shadowTokens} tok` : "—"}
      </span>
    </div>

    {#if !agentContext}
      <div class="blank">Select an agent to edit its identity.</div>
    {:else if shadowLoading}
      <div class="blank">Loading…</div>
    {:else}
      <div class="field">
        <label for="agent-payload">Identity</label>
        <textarea
          id="agent-payload"
          class="textarea"
          rows={6}
          bind:value={shadowPayload}
          oninput={() => (shadowDirty = true)}
          placeholder="Agent-specific traits, persona, specialties…"
        ></textarea>
      </div>

      {#if shadowError}<p class="err">{shadowError}</p>{/if}

      <button
        class="save"
        class:dirty={shadowDirty}
        type="button"
        disabled={!shadowDirty || shadowSaving || overBudget}
        onclick={saveShadow}
      >
        {shadowSaving ? "Saving…" : shadowDirty ? "Save" : "Saved"}
      </button>
    {/if}
  </section>
</div>

<style>
  .identity {
    display: flex;
    flex-direction: column;
    gap: var(--s4);
    padding: var(--s3);
  }

  .blank { font-size: 11px; color: var(--text-muted); padding: var(--s2) 0; }
  .err { margin: 0; font-size: 11px; color: var(--status-error); }
  .mono { font-family: "JetBrains Mono", monospace; }

  /* budget meter */
  .meter { display: flex; flex-direction: column; gap: 5px; }
  .meter .top { display: flex; justify-content: space-between; align-items: baseline; }
  .meter .lab { font-size: 11px; font-weight: 500; color: var(--text-secondary); }
  .meter .val { font-size: 10.5px; color: var(--text-primary); }
  .meter .track {
    height: 6px; background: var(--bg-sink);
    border: 1px solid var(--border-subtle); border-radius: var(--r-full); overflow: hidden;
  }
  .meter .fill { height: 100%; background: var(--accent); border-radius: var(--r-full); transition: width .2s ease; }
  .meter.warn .fill { background: var(--status-warning); }
  .meter.over .fill { background: var(--status-error); }
  .meter.warn .val { color: var(--status-warning); }
  .meter.over .val { color: var(--status-error); }
  .note { margin: 0; font-size: 10.5px; }
  .warn-note { color: var(--status-warning); }
  .over-note { color: var(--status-error); }

  /* section heads */
  .block { display: flex; flex-direction: column; gap: var(--s2); }
  .bh { display: flex; align-items: center; gap: var(--s2); }
  .bt { font-size: 10px; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; color: var(--text-muted); }
  .rule { flex: 1; height: 1px; background: var(--border-subtle); }
  .bm { font-size: 9.5px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 60%; }

  /* fields (atom spec) */
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field > label { font-size: 11px; font-weight: 500; color: var(--text-secondary); }
  .input, .textarea {
    font: inherit; font-size: 12px; color: var(--text-primary);
    background: var(--bg-raised); border: 1px solid var(--border); border-radius: var(--r-md);
    padding: 6px var(--s3); width: 100%; transition: border-color .14s, background .14s;
  }
  .input::placeholder, .textarea::placeholder { color: var(--text-muted); }
  .input:focus, .textarea:focus {
    outline: 2px solid var(--focus); outline-offset: 1px;
    border-color: var(--accent); background: var(--bg-overlay);
  }
  .textarea { resize: vertical; min-height: 80px; line-height: 1.55; }

  /* save button */
  .save {
    align-self: flex-start;
    font: inherit; font-size: 11px; font-weight: 600; cursor: pointer;
    padding: 4px var(--s3); border-radius: var(--r-md);
    background: transparent; color: var(--text-muted);
    border: 1px solid var(--border);
    transition: background .14s, color .14s, border-color .14s;
  }
  .save.dirty {
    background: var(--accent); color: var(--accent-ink); border-color: var(--accent);
  }
  .save.dirty:hover:not(:disabled) { background: var(--accent-hover); border-color: var(--accent-hover); }
  .save:disabled { cursor: default; }
  .save:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
</style>
