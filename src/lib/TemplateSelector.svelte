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
    gap: 8px;
  }

  .label {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .template-chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .template-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px 4px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 11px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .template-chip:hover {
    background: var(--bg-panel-3);
    border-color: var(--accent-border-hover);
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
    border-radius: 50%;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
  }

  .template-chip-del:hover {
    background: var(--chip-delete-hover-bg);
    color: var(--chip-delete-hover-text);
  }
</style>
