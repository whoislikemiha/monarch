<script lang="ts">
  import { invoke } from "$lib/api";
  import { open } from "@tauri-apps/plugin-dialog";
  import { matchBinding } from "$lib/keybindings.svelte";
  import type { AgentConfig, DetectedProject, ShadowGrade } from "./types";
  import { SHADOW_GRADES } from "./types";
  import type { AgentTemplateRow } from "./bindings";
  import { agentStore } from "./stores/agentStore.svelte";
  import ModelSelector from "./ModelSelector.svelte";
  import TemplateSelector from "./TemplateSelector.svelte";

  let {
    onspawn,
    oncancel,
  }: {
    onspawn: (config: AgentConfig) => void;
    oncancel: () => void;
  } = $props();

  // ModelSelector-bound state
  let provider = $state("openrouter");
  let model = $state("");
  let thinkingLevel = $state("off");
  let contextWindow: number | undefined = $state(undefined);

  let cwd = $state("/home/miha");
  let detectedProject: DetectedProject | null = $state(null);

  // Shadow identity
  let shadowName = $state("");
  let shadowTitle = $state("");
  let shadowGrade: ShadowGrade = $state("Knight");

  let saveAsTemplate = $state(false);

  function applyTemplate(t: AgentTemplateRow) {
    if (t.provider) provider = t.provider;
    // The provider $effect inside ModelSelector resets `model`, so defer
    // the model assignment (and the rest) to the next microtask so the
    // template's model survives.
    queueMicrotask(() => {
      if (t.model) model = t.model;
      if (t.thinkingLevel) thinkingLevel = t.thinkingLevel;
      if (t.cwd) cwd = t.cwd;
      shadowName = t.shadowName ?? "";
      shadowTitle = t.shadowTitle ?? "";
      if (t.shadowGrade) shadowGrade = t.shadowGrade as ShadowGrade;
    });
  }

  async function persistCurrentAsTemplate() {
    const name = shadowName.trim();
    if (!name) return;
    const now = new Date().toISOString();
    const template: AgentTemplateRow = {
      id: `tpl-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      name,
      provider,
      model: model.trim() || null,
      thinkingLevel,
      cwd: cwd || null,
      shadowName: name,
      shadowTitle: shadowTitle.trim() || null,
      shadowGrade,
      createdAt: now,
      updatedAt: now,
    };
    try {
      await invoke("db_save_agent_template", { template });
    } catch {
      // Swallow — spawning should not block on template save failures.
    }
  }

  async function detectProject(path: string) {
    try {
      detectedProject = await invoke<DetectedProject | null>("detect_project", { cwd: path });
    } catch {
      detectedProject = null;
    }
  }

  // Detect project when cwd changes
  $effect(() => {
    if (cwd.trim()) {
      detectProject(cwd.trim());
    } else {
      detectedProject = null;
    }
  });

  async function handleSpawn() {
    if (saveAsTemplate && shadowName.trim()) {
      await persistCurrentAsTemplate();
    }

    const config: AgentConfig = {
      provider,
      model: model.trim() || undefined,
      thinkingLevel: thinkingLevel !== "off" ? thinkingLevel : undefined,
      cwd: cwd || undefined,
      contextWindow,
    };

    // Attach shadow identity if a name is provided
    const sName = shadowName.trim();
    if (sName) {
      config.shadow = {
        shadowName: sName,
        shadowTitle: shadowTitle.trim() || sName,
        shadowGrade: shadowGrade,
      };
    }

    onspawn(config);
  }

  async function browseFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: cwd || undefined,
      title: "Select Working Directory",
    });
    if (selected && typeof selected === "string") {
      cwd = selected;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    // ModelSelector stops Escape propagation when its dropdown is open,
    // so here Escape is unconditionally a "close dialog" request.
    if (e.key === "Escape") oncancel();
    if (matchBinding(e, "dialog.confirm-spawn")) handleSpawn();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<TemplateSelector onselect={applyTemplate} />

<ModelSelector bind:provider bind:model bind:thinkingLevel bind:contextWindow />

{#if agentStore.projects.length > 0}
  <div class="section">
    <span class="label">Project</span>
    <div class="project-chips">
      {#each agentStore.projects as p (p.id)}
        <button
          class="project-chip"
          class:active={detectedProject?.rootPath === p.rootPath}
          onclick={() => { cwd = p.rootPath; }}
        >
          <span class="project-chip-slash">/</span>{p.name}
        </button>
      {/each}
    </div>
  </div>
{/if}

<div class="field">
  <label class="label" for="cwd">Working Directory</label>
  <div class="cwd-row">
    <input
      id="cwd"
      type="text"
      bind:value={cwd}
      placeholder="/home/miha/project"
    />
    <button class="browse-btn" onclick={browseFolder} title="Browse">
      ...
    </button>
  </div>
</div>

{#if detectedProject}
  <div class="project-info">
    <span class="project-info-label">
      <span class="project-icon">/</span>
      {detectedProject.name}
    </span>
    <span class="project-info-path">{detectedProject.rootPath}</span>
    {#if detectedProject.hasInstructions}
      <span class="project-info-tag">Project instructions found</span>
    {/if}
  </div>
{/if}

<div class="section">
  <span class="label">Shadow Identity</span>
  <div class="row">
    <div class="field">
      <label class="label" for="shadow-name">Name</label>
      <input
        id="shadow-name"
        type="text"
        bind:value={shadowName}
        placeholder="e.g. Igris, Beru, Tusk"
      />
    </div>
    <div class="field">
      <label class="label" for="shadow-grade">Grade</label>
      <select id="shadow-grade" bind:value={shadowGrade}>
        {#each SHADOW_GRADES as grade}
          <option value={grade}>{grade}</option>
        {/each}
      </select>
    </div>
  </div>
  <div class="field">
    <label class="label" for="shadow-title">Title</label>
    <input
      id="shadow-title"
      type="text"
      bind:value={shadowTitle}
      placeholder="e.g. Shadow Commander, The First Shadow"
    />
  </div>
</div>

<label class="template-save-check" title={shadowName.trim() ? "" : "Set a shadow name to enable"}>
  <input
    type="checkbox"
    bind:checked={saveAsTemplate}
    disabled={!shadowName.trim()}
  />
  <span>Save as template</span>
  {#if saveAsTemplate && shadowName.trim()}
    <span class="template-save-hint">&middot; uses "{shadowName.trim()}" as the name</span>
  {/if}
</label>

<div class="actions">
  <button class="btn-cancel" onclick={oncancel}>Cancel</button>
  <button class="btn-spawn" onclick={handleSpawn}>
    Extract
    <span class="shortcut">Ctrl+Enter</span>
  </button>
</div>

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

  .template-save-check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-secondary);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    user-select: none;
  }

  .template-save-check input[type="checkbox"] {
    width: auto;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .template-save-check input[type="checkbox"]:disabled {
    cursor: not-allowed;
  }

  .template-save-check input[type="checkbox"]:disabled + span {
    color: var(--text-muted);
  }

  .template-save-hint {
    color: var(--text-muted);
  }

  .project-chips {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .project-chip {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, color 0.15s;
  }

  .project-chip:hover {
    background: var(--bg-panel-3);
    border-color: var(--accent-border-subtle);
    color: var(--text-primary);
  }

  .project-chip.active {
    background: var(--accent-bg-hover);
    border-color: var(--accent);
    color: var(--accent);
  }

  .project-chip-slash {
    color: var(--accent);
    font-weight: 700;
  }

  .project-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--accent-border-subtle);
    background: var(--accent-bg-subtle);
  }

  .project-info-label {
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .project-info .project-icon {
    color: var(--accent);
    font-weight: 700;
  }

  .project-info-path {
    font-size: 10px;
    color: var(--text-muted);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    overflow-wrap: anywhere;
  }

  .project-info-tag {
    font-size: 10px;
    color: var(--accent);
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
  }

  .cwd-row {
    display: flex;
    gap: 6px;
  }

  .cwd-row input {
    flex: 1;
  }

  .browse-btn {
    padding: 8px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg-panel-2);
    color: var(--text-secondary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
    flex-shrink: 0;
  }

  .browse-btn:hover {
    background: var(--bg-panel-3);
  }

  .row {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }

  input,
  select {
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

  input:focus,
  select:focus {
    border-color: var(--accent);
  }

  select {
    cursor: pointer;
    appearance: none;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .btn-cancel {
    padding: 8px 16px;
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-cancel:hover {
    background: var(--bg-panel-2);
  }

  .btn-spawn {
    padding: 8px 20px;
    border: none;
    border-radius: 8px;
    background: var(--accent);
    color: var(--text-on-accent);
    font-size: 12px;
    font-weight: 600;
    font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", monospace;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    transition: background 0.15s;
  }

  .btn-spawn:hover {
    background: var(--accent-hover);
  }

  .shortcut {
    font-size: 10px;
    font-weight: 400;
    opacity: 0.6;
  }

  @media (max-width: 640px) {
    .actions {
      flex-wrap: wrap-reverse;
    }

    .actions button {
      width: 100%;
      justify-content: center;
    }
  }
</style>
