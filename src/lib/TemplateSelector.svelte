<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "$lib/api";
  import type { AgentTemplateRow } from "./bindings";

  // The parent runs its own `applyTemplate` logic on select — template
  // values need to be applied after the ModelSelector's provider-change
  // effect resets the model field, which the parent is better positioned
  // to coordinate (via queueMicrotask).
  let {
    onselect,
  }: {
    onselect: (template: AgentTemplateRow) => void;
  } = $props();

  let templates: AgentTemplateRow[] = $state([]);

  async function loadTemplates() {
    try {
      templates = await invoke<AgentTemplateRow[]>("db_list_agent_templates");
    } catch {
      templates = [];
    }
  }

  async function deleteTemplate(id: string, e: MouseEvent) {
    e.stopPropagation();
    try {
      await invoke("db_delete_agent_template", { templateId: id });
      await loadTemplates();
    } catch {}
  }

  onMount(() => {
    loadTemplates();
  });
</script>

{#if templates.length > 0}
  <div class="section">
    <span class="label">Templates</span>
    <div class="template-chips">
      {#each templates as t (t.id)}
        <button
          class="template-chip"
          onclick={() => onselect(t)}
          title={`${t.provider ?? "?"} / ${t.model ?? "?"}`}
          type="button"
        >
          <span class="template-chip-name">{t.name}</span>
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_consider_explicit_label -->
          <span
            class="template-chip-del"
            onclick={(e) => deleteTemplate(t.id, e)}
            onkeydown={(e) => { if (e.key === "Enter") deleteTemplate(t.id, e as unknown as MouseEvent); }}
            role="button"
            tabindex="0"
            title="Delete template"
          >×</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
  }

  .label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  .template-chips {
    display: flex;
    gap: var(--s2);
    flex-wrap: wrap;
  }

  .template-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: inherit;
    font-size: 11.5px;
    font-weight: 500;
    padding: 3px 5px 3px 9px;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--bg-raised);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background 0.14s, border-color 0.14s;
  }

  .template-chip:hover {
    background: var(--bg-overlay);
    border-color: var(--border-strong);
  }

  .template-chip-name {
    color: var(--text-primary);
  }

  .template-chip-del {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: var(--r-sm);
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
  }

  .template-chip-del:hover {
    background: color-mix(in srgb, var(--status-error) 14%, transparent);
    color: var(--status-error);
  }
</style>
